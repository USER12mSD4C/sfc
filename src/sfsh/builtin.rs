use crate::sfsh::ast::Command as AstCommand;
use crate::sfsh::exec::{execute_script, ExecResult};
use crate::sfsh::job::JobTable;
use crate::sfsh::vars::ShellVars;
use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

fn traps() -> &'static Mutex<HashMap<String, String>> {
    static TRAPS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    TRAPS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn run_builtin(
    name: &str,
    args: &[String],
    vars: &mut ShellVars,
    aliases: &mut HashMap<String, String>,
    funcs: &mut HashMap<String, AstCommand>,
    jobs: &mut JobTable,
    shell_pgid: i32,
) -> Option<ExecResult> {
    match name {
        "cd" => Some(ExecResult::Value(builtin_cd(args))),
        "exit" => Some(builtin_exit(args)),
        "export" => Some(ExecResult::Value(builtin_export(args, vars))),
        "unset" => Some(ExecResult::Value(builtin_unset(args, vars, aliases))),
        "alias" => Some(ExecResult::Value(builtin_alias(args, aliases))),
        "unalias" => Some(ExecResult::Value(builtin_unalias(args, aliases))),
        "source" | "." => Some(ExecResult::Value(builtin_source(
            args, vars, aliases, funcs, jobs, shell_pgid,
        ))),
        "eval" => Some(ExecResult::Value(builtin_eval(
            args, vars, aliases, funcs, jobs, shell_pgid,
        ))),
        "exec" => Some(ExecResult::Value(builtin_exec(args))),
        "set" => Some(ExecResult::Value(builtin_set(args, vars))),
        "shift" => Some(ExecResult::Value(builtin_shift(args, vars))),
        "read" => Some(ExecResult::Value(builtin_read(args, vars))),
        "local" => Some(ExecResult::Value(builtin_local(args, vars))),
        "test" | "[" => Some(ExecResult::Value(builtin_test(args))),
        "hash" => Some(ExecResult::Value(builtin_hash(args))),
        "type" => Some(ExecResult::Value(builtin_type(args, aliases))),
        "umask" => Some(ExecResult::Value(builtin_umask(args))),
        "trap" => Some(ExecResult::Value(builtin_trap(args))),
        "wait" => Some(ExecResult::Value(builtin_wait(args, jobs))),
        "fg" => Some(ExecResult::Value(builtin_fg(args, jobs))),
        "bg" => Some(ExecResult::Value(builtin_bg(args, jobs))),
        "jobs" => Some(ExecResult::Value(builtin_jobs(jobs))),
        "disown" => Some(ExecResult::Value(builtin_disown(args, jobs))),
        "return" => Some(ExecResult::Return(
            args.get(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(vars.last_status),
        )),
        "break" => Some(ExecResult::Break),
        "continue" => Some(ExecResult::Continue),
        ":" => Some(ExecResult::Value(0)),
        _ => None,
    }
}

fn builtin_cd(args: &[String]) -> i32 {
    let dest = args.get(1).map(|s| s.as_str()).unwrap_or("~");
    let path = if dest == "~" {
        env::var("HOME").unwrap_or_default()
    } else if dest == "-" {
        match env::var("OLDPWD") {
            Ok(v) => {
                println!("{}", v);
                v
            }
            Err(_) => {
                eprintln!("cd: OLDPWD not set");
                return 1;
            }
        }
    } else {
        dest.to_string()
    };
    let old = env::current_dir().ok();
    if let Err(e) = env::set_current_dir(&path) {
        eprintln!("cd: {}", e);
        return 1;
    }
    if let Some(old) = old {
        env::set_var("OLDPWD", old);
    }
    if let Ok(new) = env::current_dir() {
        env::set_var("PWD", new);
    }
    0
}

fn builtin_exit(args: &[String]) -> ExecResult {
    let code = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    std::process::exit(code);
}

fn builtin_export(args: &[String], vars: &mut ShellVars) -> i32 {
    if args.len() == 1 {
        for (k, v) in &vars.exported {
            println!("export {}=\"{}\"", k, v);
        }
        return 0;
    }
    for arg in &args[1..] {
        if let Some((k, v)) = arg.split_once('=') {
            vars.set(k, v, true);
        } else {
            vars.export(arg);
        }
    }
    0
}

fn builtin_unset(
    args: &[String],
    vars: &mut ShellVars,
    aliases: &mut HashMap<String, String>,
) -> i32 {
    for arg in &args[1..] {
        vars.unset(arg);
        aliases.remove(arg);
    }
    0
}

fn builtin_alias(args: &[String], aliases: &mut HashMap<String, String>) -> i32 {
    if args.len() == 1 {
        for (k, v) in aliases.iter() {
            println!("{}='{}'", k, v);
        }
        return 0;
    }
    for arg in &args[1..] {
        if let Some((k, v)) = arg.split_once('=') {
            aliases.insert(
                k.to_string(),
                v.trim_matches('"').trim_matches('\'').to_string(),
            );
        } else if let Some(v) = aliases.get(arg) {
            println!("{}='{}'", arg, v);
        }
    }
    0
}

fn builtin_unalias(args: &[String], aliases: &mut HashMap<String, String>) -> i32 {
    if args.get(1).map(|s| s == "-a").unwrap_or(false) {
        aliases.clear();
        return 0;
    }
    for arg in &args[1..] {
        aliases.remove(arg);
    }
    0
}

fn builtin_source(
    args: &[String],
    vars: &mut ShellVars,
    aliases: &mut HashMap<String, String>,
    funcs: &mut HashMap<String, AstCommand>,
    jobs: &mut JobTable,
    shell_pgid: i32,
) -> i32 {
    let file = match args.get(1) {
        Some(f) => f,
        None => {
            eprintln!("source: filename required");
            return 1;
        }
    };
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("source: {}", e);
            return 1;
        }
    };
    execute_script(&content, vars, aliases, funcs, jobs, shell_pgid)
}

fn builtin_eval(
    args: &[String],
    vars: &mut ShellVars,
    aliases: &mut HashMap<String, String>,
    funcs: &mut HashMap<String, AstCommand>,
    jobs: &mut JobTable,
    shell_pgid: i32,
) -> i32 {
    let line = args[1..].join(" ");
    execute_script(&line, vars, aliases, funcs, jobs, shell_pgid)
}

fn builtin_exec(args: &[String]) -> i32 {
    if args.len() < 2 {
        return 0;
    }
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(&args[1]).args(&args[2..]).exec();
    eprintln!("exec: {}", err);
    1
}

fn builtin_set(args: &[String], vars: &mut ShellVars) -> i32 {
    if args.len() == 1 {
        for (k, v) in &vars.vars {
            println!("{}={}", k, v);
        }
        for (k, v) in &vars.exported {
            println!("{}={}", k, v);
        }
        return 0;
    }
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            vars.set_positional(args[i + 1..].to_vec());
            return 0;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            for c in arg[1..].chars() {
                match c {
                    'e' => vars.set_opt('e', true),
                    'u' => vars.set_opt('u', true),
                    'x' => vars.set_opt('x', true),
                    _ => {}
                }
            }
        } else if arg.starts_with('+') && arg.len() > 1 {
            for c in arg[1..].chars() {
                match c {
                    'e' => vars.set_opt('e', false),
                    'u' => vars.set_opt('u', false),
                    'x' => vars.set_opt('x', false),
                    _ => {}
                }
            }
        }
        i += 1;
    }
    0
}

fn builtin_shift(args: &[String], vars: &mut ShellVars) -> i32 {
    let n = args
        .get(1)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    if n > vars.positional.len() {
        eprintln!("shift: can't shift that many");
        return 1;
    }
    vars.positional = vars.positional[n..].to_vec();
    0
}

fn builtin_read(args: &[String], vars: &mut ShellVars) -> i32 {
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return 1;
    }
    if line.ends_with('\n') {
        line.pop();
    }
    let name = args.get(1).map(|s| s.as_str()).unwrap_or("REPLY");
    vars.set(name, &line, false);
    0
}

fn builtin_local(args: &[String], vars: &mut ShellVars) -> i32 {
    for arg in &args[1..] {
        if let Some((k, v)) = arg.split_once('=') {
            vars.local_set(k, v);
        } else {
            vars.local_set(arg, "");
        }
    }
    0
}

fn builtin_test(args: &[String]) -> i32 {
    if args.len() == 1 {
        return 1;
    }
    test_expr(&args[1..])
}

fn test_expr(args: &[String]) -> i32 {
    if args.is_empty() {
        return 1;
    }
    if args[0] == "!" {
        let r = test_expr(&args[1..]);
        return if r == 0 { 1 } else { 0 };
    }
    if args[0] == "(" {
        let mut depth = 1;
        let mut end = 1;
        while end < args.len() && depth > 0 {
            if args[end] == "(" {
                depth += 1;
            } else if args[end] == ")" {
                depth -= 1;
            }
            end += 1;
        }
        let r = test_expr(&args[1..end - 1]);
        if end < args.len() {
            if args[end] == "-a" {
                return if r == 0 && test_expr(&args[end + 1..]) == 0 {
                    0
                } else {
                    1
                };
            }
            if args[end] == "-o" {
                return if r == 0 || test_expr(&args[end + 1..]) == 0 {
                    0
                } else {
                    1
                };
            }
        }
        return r;
    }
    if args.len() == 1 {
        return if args[0].is_empty() { 1 } else { 0 };
    }
    if args.len() == 2 {
        let a = &args[1];
        match args[0].as_str() {
            "-z" => return if a.is_empty() { 0 } else { 1 },
            "-n" => return if a.is_empty() { 1 } else { 0 },
            "-e" => return if Path::new(a).exists() { 0 } else { 1 },
            "-f" => return if Path::new(a).is_file() { 0 } else { 1 },
            "-d" => return if Path::new(a).is_dir() { 0 } else { 1 },
            "-h" | "-L" => {
                return if std::fs::symlink_metadata(a)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false)
                {
                    0
                } else {
                    1
                }
            }
            "-s" => {
                return if std::fs::metadata(a).map(|m| m.len() > 0).unwrap_or(false) {
                    0
                } else {
                    1
                }
            }
            "-t" => {
                return if a
                    .parse::<i32>()
                    .map(|fd| unsafe { libc::isatty(fd) != 0 })
                    .unwrap_or(false)
                {
                    0
                } else {
                    1
                }
            }
            _ => {}
        }
    }
    if args.len() == 3 {
        let (x, op, y) = (&args[0], &args[1], &args[2]);
        match op.as_str() {
            "=" => return if x == y { 0 } else { 1 },
            "!=" => return if x != y { 0 } else { 1 },
            "-eq" => {
                let a = x.parse::<i64>().unwrap_or(0);
                let b = y.parse::<i64>().unwrap_or(0);
                return if a == b { 0 } else { 1 };
            }
            "-ne" => {
                let a = x.parse::<i64>().unwrap_or(0);
                let b = y.parse::<i64>().unwrap_or(0);
                return if a != b { 0 } else { 1 };
            }
            "-gt" => {
                let a = x.parse::<i64>().unwrap_or(0);
                let b = y.parse::<i64>().unwrap_or(0);
                return if a > b { 0 } else { 1 };
            }
            "-ge" => {
                let a = x.parse::<i64>().unwrap_or(0);
                let b = y.parse::<i64>().unwrap_or(0);
                return if a >= b { 0 } else { 1 };
            }
            "-lt" => {
                let a = x.parse::<i64>().unwrap_or(0);
                let b = y.parse::<i64>().unwrap_or(0);
                return if a < b { 0 } else { 1 };
            }
            "-le" => {
                let a = x.parse::<i64>().unwrap_or(0);
                let b = y.parse::<i64>().unwrap_or(0);
                return if a <= b { 0 } else { 1 };
            }
            _ => {}
        }
    }
    if args.len() >= 3 {
        if args[1] == "-a" {
            return if test_expr(&args[0..1]) == 0 && test_expr(&args[2..]) == 0 {
                0
            } else {
                1
            };
        }
        if args[1] == "-o" {
            return if test_expr(&args[0..1]) == 0 || test_expr(&args[2..]) == 0 {
                0
            } else {
                1
            };
        }
    }
    1
}

fn builtin_hash(_args: &[String]) -> i32 {
    0
}

fn builtin_type(args: &[String], aliases: &HashMap<String, String>) -> i32 {
    for arg in &args[1..] {
        if aliases.contains_key(arg) {
            println!("{} is an alias for {}", arg, aliases.get(arg).unwrap());
            continue;
        }
        if is_shell_builtin(arg) {
            println!("{} is a shell builtin", arg);
            continue;
        }
        if let Ok(path) = which(arg) {
            println!("{} is {}", arg, path);
        } else {
            eprintln!("type: {}: not found", arg);
        }
    }
    0
}

fn is_shell_builtin(name: &str) -> bool {
    matches!(
        name,
        "cd" | "exit"
            | "export"
            | "unset"
            | "alias"
            | "unalias"
            | "source"
            | "."
            | "eval"
            | "exec"
            | "set"
            | "shift"
            | "read"
            | "local"
            | "test"
            | "["
            | "hash"
            | "type"
            | "umask"
            | "trap"
            | "wait"
            | "fg"
            | "bg"
            | "jobs"
            | "disown"
            | "return"
            | "break"
            | "continue"
            | ":"
    )
}

fn which(cmd: &str) -> Result<String, ()> {
    if let Ok(path) = env::var("PATH") {
        for dir in env::split_paths(&path) {
            let p = dir.join(cmd);
            if p.exists() {
                return Ok(p.to_string_lossy().to_string());
            }
        }
    }
    Err(())
}

fn builtin_umask(args: &[String]) -> i32 {
    if args.len() == 1 {
        let mask = unsafe { libc::umask(0) };
        unsafe {
            libc::umask(mask);
        }
        println!("{:04o}", mask);
        return 0;
    }
    if args[1] == "-S" {
        let mask = unsafe { libc::umask(0) };
        unsafe {
            libc::umask(mask);
        }
        let mut perms = String::new();
        let u = ((mask >> 6) & 0o7) ^ 0o7;
        let g = ((mask >> 3) & 0o7) ^ 0o7;
        let o = (mask & 0o7) ^ 0o7;
        for (val, chars) in [(u, "rwx"), (g, "rwx"), (o, "rwx")] {
            for (i, c) in chars.chars().enumerate() {
                if val & (4 >> i) != 0 {
                    perms.push(c);
                } else {
                    perms.push('-');
                }
            }
        }
        println!("u={},g={},o={}", &perms[0..3], &perms[3..6], &perms[6..9]);
        return 0;
    }
    let mode = if let Ok(m) = u32::from_str_radix(&args[1], 8) {
        m
    } else {
        eprintln!("umask: invalid mode");
        return 1;
    };
    unsafe {
        libc::umask(mode);
    }
    0
}

fn builtin_trap(args: &[String]) -> i32 {
    if args.len() == 1 {
        let t = traps().lock().unwrap();
        for (sig, cmd) in t.iter() {
            println!("trap -- '{}' {}", cmd, sig);
        }
        return 0;
    }
    let sig = args.last().unwrap().clone();
    if args[1] == "-" {
        traps().lock().unwrap().remove(&sig);
        return 0;
    }
    let cmd = args[1..args.len() - 1].join(" ");
    traps().lock().unwrap().insert(sig, cmd);
    0
}

fn builtin_wait(_args: &[String], _jobs: &mut JobTable) -> i32 {
    0
}

fn builtin_fg(args: &[String], jobs: &mut JobTable) -> i32 {
    let id = if args.len() == 1 {
        jobs.jobs.keys().copied().max().unwrap_or(0)
    } else {
        let spec = &args[1];
        if spec.starts_with('%') {
            spec[1..].parse().unwrap_or(0)
        } else {
            spec.parse().unwrap_or(0)
        }
    };
    if let Some(job) = jobs.jobs.get(&id) {
        let pgid = job.pgid;
        jobs.current_pgid = Some(pgid);
        unsafe {
            libc::tcsetpgrp(0, pgid.as_raw());
        }
        nix::sys::signal::kill(pgid, nix::sys::signal::Signal::SIGCONT).ok();
        0
    } else {
        eprintln!(
            "fg: {}: no such job",
            args.get(1).map(|s| s.as_str()).unwrap_or("current")
        );
        1
    }
}

fn builtin_bg(args: &[String], jobs: &mut JobTable) -> i32 {
    let id = if args.len() == 1 {
        jobs.jobs.keys().copied().max().unwrap_or(0)
    } else {
        let spec = &args[1];
        if spec.starts_with('%') {
            spec[1..].parse().unwrap_or(0)
        } else {
            spec.parse().unwrap_or(0)
        }
    };
    jobs.background(id);
    0
}

fn builtin_jobs(jobs: &mut JobTable) -> i32 {
    for (id, job) in jobs.jobs.iter() {
        let status = if job.stopped {
            "Stopped"
        } else if job.done {
            "Done"
        } else {
            "Running"
        };
        println!("[{}] {} {}", id, status, job.cmd);
    }
    0
}

fn builtin_disown(args: &[String], jobs: &mut JobTable) -> i32 {
    if args.len() == 1 {
        if let Some(max_id) = jobs.jobs.keys().copied().max() {
            jobs.jobs.remove(&max_id);
        }
    } else if args[1] == "-a" {
        jobs.jobs.clear();
    } else {
        let spec = &args[1];
        let id = if spec.starts_with('%') {
            spec[1..].parse().unwrap_or(0)
        } else {
            spec.parse().unwrap_or(0)
        };
        jobs.jobs.remove(&id);
    }
    0
}
