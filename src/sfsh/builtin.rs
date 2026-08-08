use crate::sfsh::ast::Command as AstCommand;
use crate::sfsh::exec::{execute_script, ExecResult};
use crate::sfsh::job::JobTable;
use crate::sfsh::vars::ShellVars;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::env;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

fn traps() -> &'static Mutex<HashMap<String, String>> {
    static TRAPS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    TRAPS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');

    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }

    out.push('\'');
    out
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
        "cd" => Some(ExecResult::Value(builtin_cd(args, vars))),
        "exit" => Some(builtin_exit(args)),
        "export" => Some(ExecResult::Value(builtin_export(args, vars))),
        "unset" => Some(ExecResult::Value(builtin_unset(args, vars, aliases, funcs))),
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
        "hash" => Some(ExecResult::Value(0)),
        "type" => Some(ExecResult::Value(builtin_type(args, &*aliases, &*funcs))),
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
        "echo" => Some(ExecResult::Value(builtin_echo(args))),
        "true" => Some(ExecResult::Value(0)),
        "false" => Some(ExecResult::Value(1)),
        "printf" => Some(ExecResult::Value(builtin_printf(args))),
        "pwd" => Some(ExecResult::Value(builtin_pwd())),
        "command" => Some(ExecResult::Value(builtin_command(
            args, vars, aliases, funcs, jobs, shell_pgid,
        ))),
        "readonly" => Some(ExecResult::Value(builtin_readonly(args, vars))),
        "times" => Some(ExecResult::Value(builtin_times())),
        "getopts" => Some(ExecResult::Value(builtin_getopts(args, vars))),
        "lem" => Some(ExecResult::Value(builtin_lem(args))),
        _ => None,
    }
}

fn builtin_cd(args: &[String], vars: &mut ShellVars) -> i32 {
    let dest = args.get(1).map(|s| s.as_str());
    let path = match dest {
        None | Some("~") => env::var("HOME").unwrap_or_default(),
        Some("-") => {
            let old = vars.get("OLDPWD").unwrap_or_default();
            if old.is_empty() {
                eprintln!("cd: OLDPWD not set");
                return 1;
            }
            println!("{}", old);
            old
        }
        Some(p) => p.to_string(),
    };
    let old = env::current_dir().ok();
    if let Err(e) = env::set_current_dir(&path) {
        eprintln!("cd: {}", e);
        return 1;
    }
    if let Some(old) = old {
        vars.set("OLDPWD", &old.to_string_lossy(), true);
    }
    if let Ok(new) = env::current_dir() {
        vars.set("PWD", &new.to_string_lossy(), true);
    }
    0
}

fn builtin_exit(args: &[String]) -> ExecResult {
    let code = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    ExecResult::Exit(code)
}

fn builtin_export(args: &[String], vars: &mut ShellVars) -> i32 {
    if args.len() == 1 {
        let mut keys: Vec<&String> = vars.exported.keys().collect();
        keys.sort();
        for k in keys {
            if let Some(v) = vars.exported.get(k) {
                println!("export {}={}", k, shell_quote(v));
            }
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
    _aliases: &mut HashMap<String, String>,
    funcs: &mut HashMap<String, AstCommand>,
) -> i32 {
    let mut unset_funcs = false;
    let mut unset_vars = true;
    let mut targets: Vec<&str> = Vec::new();
    let mut opts_end = false;

    for arg in &args[1..] {
        if opts_end {
            targets.push(arg.as_str());
            continue;
        }

        match arg.as_str() {
            "--" => {
                opts_end = true;
            }
            "-f" => {
                unset_funcs = true;
                unset_vars = false;
            }
            "-v" => {
                unset_vars = true;
                unset_funcs = false;
            }
            _ if arg.starts_with('-') && arg.len() > 1 => {
                eprintln!("unset: invalid option: {}", arg);
                return 2;
            }
            _ => {
                targets.push(arg.as_str());
            }
        }
    }

    for t in targets {
        if unset_vars {
            vars.unset(t);
        }

        if unset_funcs {
            funcs.remove(t);
        }
    }

    0
}

fn builtin_alias(args: &[String], aliases: &mut HashMap<String, String>) -> i32 {
    if args.len() == 1 {
        let mut keys: Vec<&String> = aliases.keys().collect();
        keys.sort();

        for k in keys {
            if let Some(v) = aliases.get(k) {
                println!("alias {}={}", k, shell_quote(v));
            }
        }

        return 0;
    }

    let mut rc = 0;

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];

        if let Some((k, v)) = arg.split_once('=') {
            aliases.insert(k.to_string(), v.to_string());
            i += 1;
        } else if i + 1 < args.len() && args[i + 1].starts_with('=') {
            let v = &args[i + 1][1..];
            aliases.insert(arg.to_string(), v.to_string());
            i += 2;
        } else if let Some(v) = aliases.get(arg) {
            println!("alias {}={}", arg, shell_quote(v));
            i += 1;
        } else {
            eprintln!("alias: {}: not found", arg);
            rc = 1;
            i += 1;
        }
    }

    rc
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
    if args.len() < 2 {
        return 0;
    }
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
        let mut all: Vec<(&String, &String)> = Vec::new();
        for (k, v) in &vars.vars {
            all.push((k, v));
        }
        for (k, v) in &vars.exported {
            if !vars.vars.contains_key(k) {
                all.push((k, v));
            }
        }
        all.sort_by(|a, b| a.0.cmp(b.0));
        for (k, v) in all {
            println!("{}={}", k, v);
        }
        return 0;
    }
    let mut i = 1;
    let mut positional: Option<Vec<String>> = None;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            positional = Some(args[i + 1..].to_vec());
            break;
        }
        if arg == "-" {
            positional = Some(args[i + 1..].to_vec());
            break;
        }
        if (arg.starts_with('-') || arg.starts_with('+')) && arg.len() > 1 {
            let enable = arg.starts_with('-');
            let bytes = arg.as_bytes();
            let mut j = 1;
            while j < bytes.len() {
                let c = bytes[j] as char;
                if c == 'o' {
                    let opt_name = if j + 1 < bytes.len() {
                        &arg[j + 1..]
                    } else {
                        i += 1;
                        if i < args.len() {
                            args[i].as_str()
                        } else {
                            ""
                        }
                    };
                    match opt_name {
                        "pipefail" => vars.set_opt('p', enable),
                        "errexit" => vars.set_opt('e', enable),
                        "nounset" => vars.set_opt('u', enable),
                        "xtrace" => vars.set_opt('x', enable),
                        "noglob" => vars.set_opt('f', enable),
                        "errtrace" => vars.set_opt('E', enable),
                        _ => {}
                    }
                    break;
                } else {
                    match c {
                        'e' => vars.set_opt('e', enable),
                        'u' => vars.set_opt('u', enable),
                        'x' => vars.set_opt('x', enable),
                        'f' => vars.set_opt('f', enable),
                        'E' => vars.set_opt('E', enable),
                        'p' => vars.set_opt('p', enable),
                        _ => {}
                    }
                }
                j += 1;
            }
        } else {
            positional = Some(args[i..].to_vec());
            break;
        }
        i += 1;
    }
    if let Some(p) = positional {
        vars.set_positional(p);
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
    let mut names: Vec<String> = Vec::new();
    let mut raw = false;
    let mut prompt: Option<String> = None;
    let mut n_chars: Option<usize> = None;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "-r" => raw = true,
            "-p" => {
                i += 1;
                prompt = args.get(i).cloned();
            }
            "-n" => {
                i += 1;
                n_chars = args.get(i).and_then(|s| s.parse().ok());
            }
            _ if args[i].starts_with("-p") => {
                prompt = Some(args[i][2..].to_string());
            }
            _ if args[i].starts_with("-n") => {
                n_chars = args[i][2..].parse().ok();
            }
            _ => {
                names.push(args[i].clone());
            }
        }

        i += 1;
    }

    if let Some(p) = &prompt {
        eprint!("{}", p);
        let _ = std::io::Write::flush(&mut std::io::stderr());
    }

    if names.is_empty() {
        names.push("REPLY".to_string());
    }

    let mut line = String::new();

    match std::io::stdin().read_line(&mut line) {
        Ok(0) => return 1,
        Ok(_) => {}
        Err(_) => return 1,
    }

    if line.ends_with('\n') {
        line.pop();
    }

    if !raw {
        let mut result = String::new();
        let mut chars = line.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(&next) = chars.peek() {
                    if next == '\n' {
                        chars.next();
                        let mut line2 = String::new();
                        if std::io::stdin().read_line(&mut line2).is_ok() {
                            result.push_str(&line2);
                        }
                        continue;
                    }
                }
            }
            result.push(c);
        }

        line = result;
    }

    if let Some(n) = n_chars {
        line = line.chars().take(n).collect();
    }

    let ifs = vars.get("IFS").unwrap_or_else(|| " \t\n".to_string());

    if names.len() == 1 {
        vars.set(&names[0], &line, false);
    } else {
        let fields: Vec<&str> = if ifs.is_empty() {
            vec![line.as_str()]
        } else {
            line.split(|c: char| ifs.contains(c))
                .filter(|s| !s.is_empty())
                .collect()
        };

        for (i, name) in names.iter().enumerate() {
            if i < names.len() - 1 {
                let val = fields.get(i).copied().unwrap_or("");
                vars.set(name, val, false);
            } else {
                let val = if i < fields.len() {
                    fields[i..].join(" ")
                } else {
                    String::new()
                };
                vars.set(name, &val, false);
            }
        }
    }

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
    let test_args: Vec<String> = if args.get(0).map(|s| s == "[").unwrap_or(false) {
        if args.last().map(|s| s == "]").unwrap_or(false) {
            args[1..args.len() - 1].to_vec()
        } else {
            eprintln!("[: missing ]");
            return 2;
        }
    } else {
        args[1..].to_vec()
    };
    if test_args.is_empty() {
        return 1;
    }
    test_expr(&test_args)
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
        use std::os::unix::fs::MetadataExt;

        let a = &args[1];

        let access_ok = |path: &str, mode: libc::c_int| -> bool {
            match std::ffi::CString::new(path) {
                Ok(c) => unsafe { libc::access(c.as_ptr(), mode) == 0 },
                Err(_) => false,
            }
        };

        let mode_of = |path: &str| -> Option<u32> {
            std::fs::metadata(path)
                .ok()
                .map(|m| m.permissions().mode())
        };

        match args[0].as_str() {
            "-z" => return if a.is_empty() { 0 } else { 1 },
            "-n" => return if a.is_empty() { 1 } else { 0 },
            "-e" => return if std::path::Path::new(a).exists() { 0 } else { 1 },
            "-f" => return if std::path::Path::new(a).is_file() { 0 } else { 1 },
            "-d" => return if std::path::Path::new(a).is_dir() { 0 } else { 1 },
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
            "-r" => return if access_ok(a, libc::R_OK) { 0 } else { 1 },
            "-w" => return if access_ok(a, libc::W_OK) { 0 } else { 1 },
            "-x" => return if access_ok(a, libc::X_OK) { 0 } else { 1 },
            "-b" => {
                return if mode_of(a)
                    .map(|mode| (mode & libc::S_IFMT) == libc::S_IFBLK)
                    .unwrap_or(false)
                {
                    0
                } else {
                    1
                }
            }
            "-c" => {
                return if mode_of(a)
                    .map(|mode| (mode & libc::S_IFMT) == libc::S_IFCHR)
                    .unwrap_or(false)
                {
                    0
                } else {
                    1
                }
            }
            "-p" => {
                return if mode_of(a)
                    .map(|mode| (mode & libc::S_IFMT) == libc::S_IFIFO)
                    .unwrap_or(false)
                {
                    0
                } else {
                    1
                }
            }
            "-S" => {
                return if mode_of(a)
                    .map(|mode| (mode & libc::S_IFMT) == libc::S_IFSOCK)
                    .unwrap_or(false)
                {
                    0
                } else {
                    1
                }
            }
            "-u" => {
                return if mode_of(a)
                    .map(|mode| mode & libc::S_ISUID != 0)
                    .unwrap_or(false)
                {
                    0
                } else {
                    1
                }
            }
            "-g" => {
                return if mode_of(a)
                    .map(|mode| mode & libc::S_ISGID != 0)
                    .unwrap_or(false)
                {
                    0
                } else {
                    1
                }
            }
            "-k" => {
                return if mode_of(a)
                    .map(|mode| mode & libc::S_ISVTX != 0)
                    .unwrap_or(false)
                {
                    0
                } else {
                    1
                }
            }
            "-G" => {
                return if std::fs::metadata(a)
                    .ok()
                    .map(|m| m.gid() == unsafe { libc::getegid() })
                    .unwrap_or(false)
                {
                    0
                } else {
                    1
                }
            }
            "-O" => {
                return if std::fs::metadata(a)
                    .ok()
                    .map(|m| m.uid() == unsafe { libc::geteuid() })
                    .unwrap_or(false)
                {
                    0
                } else {
                    1
                }
            }
            "-N" => {
                return if std::fs::metadata(a)
                    .ok()
                    .map(|m| m.mtime() > m.atime())
                    .unwrap_or(false)
                {
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
            _ => {
                eprintln!("test: unknown operator: {}", args[0]);
                return 2;
            }
        }
    }

    if args.len() == 3 {
        use std::os::unix::fs::MetadataExt;

        let mtime_of = |path: &str| -> Option<std::time::SystemTime> {
            std::fs::metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
        };

        let file_newer = |a: &str, b: &str| -> bool {
            match (mtime_of(a), mtime_of(b)) {
                (Some(ta), Some(tb)) => ta > tb,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => false,
            }
        };

        let same_file = |a: &str, b: &str| -> bool {
            match (std::fs::metadata(a).ok(), std::fs::metadata(b).ok()) {
                (Some(ma), Some(mb)) => ma.dev() == mb.dev() && ma.ino() == mb.ino(),
                _ => false,
            }
        };

        let parse_int = |s: &str| -> Option<i64> { s.parse::<i64>().ok() };

        let (x, op, y) = (&args[0], &args[1], &args[2]);

        match op.as_str() {
            "=" | "==" => return if x == y { 0 } else { 1 },
            "!=" => return if x != y { 0 } else { 1 },
            "<" => return if x < y { 0 } else { 1 },
            ">" => return if x > y { 0 } else { 1 },
            "-nt" => return if file_newer(x, y) { 0 } else { 1 },
            "-ot" => return if file_newer(y, x) { 0 } else { 1 },
            "-ef" => return if same_file(x, y) { 0 } else { 1 },
            "-eq" => {
                return match (parse_int(x), parse_int(y)) {
                    (Some(a), Some(b)) if a == b => 0,
                    (Some(_), Some(_)) => 1,
                    _ => 2,
                }
            }
            "-ne" => {
                return match (parse_int(x), parse_int(y)) {
                    (Some(a), Some(b)) if a != b => 0,
                    (Some(_), Some(_)) => 1,
                    _ => 2,
                }
            }
            "-gt" => {
                return match (parse_int(x), parse_int(y)) {
                    (Some(a), Some(b)) if a > b => 0,
                    (Some(_), Some(_)) => 1,
                    _ => 2,
                }
            }
            "-ge" => {
                return match (parse_int(x), parse_int(y)) {
                    (Some(a), Some(b)) if a >= b => 0,
                    (Some(_), Some(_)) => 1,
                    _ => 2,
                }
            }
            "-lt" => {
                return match (parse_int(x), parse_int(y)) {
                    (Some(a), Some(b)) if a < b => 0,
                    (Some(_), Some(_)) => 1,
                    _ => 2,
                }
            }
            "-le" => {
                return match (parse_int(x), parse_int(y)) {
                    (Some(a), Some(b)) if a <= b => 0,
                    (Some(_), Some(_)) => 1,
                    _ => 2,
                }
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

fn builtin_type(
    args: &[String],
    aliases: &HashMap<String, String>,
    funcs: &HashMap<String, AstCommand>,
) -> i32 {
    let mut rc = 0;

    for arg in &args[1..] {
        if let Some(alias) = aliases.get(arg) {
            println!("{} is an alias for {}", arg, alias);
        } else if funcs.contains_key(arg) {
            println!("{} is a shell function", arg);
        } else if is_shell_builtin(arg) {
            println!("{} is a shell builtin", arg);
        } else if let Ok(path) = which(arg) {
            println!("{} is {}", arg, path);
        } else {
            eprintln!("type: {}: not found", arg);
            rc = 1;
        }
    }

    rc
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
            | "echo"
            | "true"
            | "false"
            | "printf"
            | "pwd"
            | "command"
            | "readonly"
            | "times"
            | "getopts"
            | "lem"
    )
}

fn which(cmd: &str) -> Result<String, ()> {
    if cmd.contains('/') {
        let p = Path::new(cmd);
        if p.is_file() {
            return Ok(cmd.to_string());
        }
        return Err(());
    }
    if let Ok(path) = env::var("PATH") {
        for dir in env::split_paths(&path) {
            let p = dir.join(cmd);
            if p.is_file() {
                if let Ok(meta) = std::fs::metadata(&p) {
                    if meta.permissions().mode() & 0o111 != 0 {
                        return Ok(p.to_string_lossy().to_string());
                    }
                }
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
        let u = ((mask >> 6) & 0o7) ^ 0o7;
        let g = ((mask >> 3) & 0o7) ^ 0o7;
        let o = (mask & 0o7) ^ 0o7;
        let perm_str = |val: u32| -> String {
            let mut s = String::new();
            s.push(if val & 4 != 0 { 'r' } else { '-' });
            s.push(if val & 2 != 0 { 'w' } else { '-' });
            s.push(if val & 1 != 0 { 'x' } else { '-' });
            s
        };
        println!("u={},g={},o={}", perm_str(u), perm_str(g), perm_str(o));
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
        let mut keys: Vec<&String> = t.keys().collect();
        keys.sort();
        for sig in keys {
            println!("trap -- '{}' {}", t.get(sig).unwrap(), sig);
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

fn builtin_wait(args: &[String], jobs: &mut JobTable) -> i32 {
    if args.len() == 1 {
        loop {
            match waitpid(None, Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::StillAlive) => {
                    if jobs.jobs.is_empty() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }
                Ok(_) => continue,
                Err(nix::errno::Errno::ECHILD) => break,
                Err(_) => break,
            }
        }
        0
    } else {
        let spec = &args[1];
        if spec.starts_with('%') {
            let id: usize = spec[1..].parse().unwrap_or(0);
            if let Some(job) = jobs.jobs.get(&id) {
                let pgid = job.pgid;
                loop {
                    match waitpid(pgid, None) {
                        Ok(_) => break,
                        Err(nix::errno::Errno::EINTR) => continue,
                        Err(_) => break,
                    }
                }
            }
        } else {
            let pid = spec.parse::<i32>().unwrap_or(-1);
            if pid > 0 {
                let _ = waitpid(Pid::from_raw(pid), None);
            }
        }
        0
    }
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
        println!("{}", job.cmd);
        unsafe {
            libc::tcsetpgrp(0, pgid.as_raw());
        }
        nix::sys::signal::kill(pgid, nix::sys::signal::Signal::SIGCONT).ok();
        loop {
            match waitpid(pgid, Some(WaitPidFlag::WUNTRACED)) {
                Ok(WaitStatus::Exited(_, code)) => {
                    jobs.jobs.remove(&id);
                    unsafe {
                        libc::tcsetpgrp(0, libc::getpgrp());
                    }
                    return code;
                }
                Ok(WaitStatus::Signaled(_, sig, _)) => {
                    jobs.jobs.remove(&id);
                    unsafe {
                        libc::tcsetpgrp(0, libc::getpgrp());
                    }
                    return 128 + sig as i32;
                }
                Ok(WaitStatus::Stopped(_, _)) => {
                    if let Some(job) = jobs.jobs.get_mut(&id) {
                        job.stopped = true;
                    }
                    unsafe {
                        libc::tcsetpgrp(0, libc::getpgrp());
                    }
                    return 0;
                }
                Err(nix::errno::Errno::EINTR) => continue,
                Err(_) => break,
                _ => break,
            }
        }
        unsafe {
            libc::tcsetpgrp(0, libc::getpgrp());
        }
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
    if jobs.background(id).is_some() {
        0
    } else {
        eprintln!("bg: no such job");
        1
    }
}

fn builtin_jobs(jobs: &mut JobTable) -> i32 {
    let mut ids: Vec<usize> = jobs.jobs.keys().copied().collect();
    ids.sort();
    for id in ids {
        if let Some(job) = jobs.jobs.get(&id) {
            let status = if job.stopped {
                "Stopped"
            } else if job.done {
                "Done"
            } else {
                "Running"
            };
            println!("[{}] {} {}", id, status, job.cmd);
        }
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

fn builtin_echo(args: &[String]) -> i32 {
    let mut stdout = std::io::stdout().lock();
    let mut iter = args.iter().skip(1).peekable();
    let mut newline = true;

    while let Some(first) = iter.peek() {
        if *first == "-n" {
            newline = false;
            iter.next();
        } else {
            break;
        }
    }

    while let Some(arg) = iter.next() {
        let _ = std::io::Write::write_all(&mut stdout, arg.as_bytes());

        if iter.peek().is_some() {
            let _ = std::io::Write::write_all(&mut stdout, b" ");
        }
    }

    if newline {
        let _ = std::io::Write::write_all(&mut stdout, b"\n");
    }

    let _ = std::io::Write::flush(&mut stdout);

    0
}

fn builtin_printf(args: &[String]) -> i32 {
    if args.len() < 2 {
        eprintln!("printf: usage: printf format [arguments...]");
        return 1;
    }
    let fmt = &args[1];
    let fmt_args = &args[2..];
    let mut arg_idx = 0;
    let mut output = String::new();

    loop {
        let mut consumed_arg = false;
        let mut chars = fmt.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '%' {
                let mut spec = String::new();
                spec.push('%');
                while let Some(&next) = chars.peek() {
                    if next == '-' || next == '+' || next == ' ' || next == '#' || next == '0' {
                        spec.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_digit() {
                        spec.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if chars.peek() == Some(&'.') {
                    spec.push('.');
                    chars.next();
                    while let Some(&next) = chars.peek() {
                        if next.is_ascii_digit() {
                            spec.push(next);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                match chars.next() {
                    Some('s') => {
                        let val = fmt_args.get(arg_idx).map(|s| s.as_str()).unwrap_or("");
                        output.push_str(val);
                        arg_idx += 1;
                        consumed_arg = true;
                    }
                    Some('d') | Some('i') => {
                        let val = fmt_args
                            .get(arg_idx)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        output.push_str(&val.to_string());
                        arg_idx += 1;
                        consumed_arg = true;
                    }
                    Some('x') => {
                        let val = fmt_args
                            .get(arg_idx)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        output.push_str(&format!("{:x}", val));
                        arg_idx += 1;
                        consumed_arg = true;
                    }
                    Some('X') => {
                        let val = fmt_args
                            .get(arg_idx)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        output.push_str(&format!("{:X}", val));
                        arg_idx += 1;
                        consumed_arg = true;
                    }
                    Some('o') => {
                        let val = fmt_args
                            .get(arg_idx)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        output.push_str(&format!("{:o}", val));
                        arg_idx += 1;
                        consumed_arg = true;
                    }
                    Some('c') => {
                        let val = fmt_args.get(arg_idx).map(|s| s.as_str()).unwrap_or("");
                        if let Some(c) = val.chars().next() {
                            output.push(c);
                        }
                        arg_idx += 1;
                        consumed_arg = true;
                    }
                    Some('%') => output.push('%'),
                    Some(c2) => {
                        output.push('%');
                        output.push(c2);
                    }
                    None => output.push('%'),
                }
            } else if c == '\\' {
                match chars.next() {
                    Some('n') => output.push('\n'),
                    Some('t') => output.push('\t'),
                    Some('r') => output.push('\r'),
                    Some('\\') => output.push('\\'),
                    Some('a') => output.push('\x07'),
                    Some('b') => output.push('\x08'),
                    Some('f') => output.push('\x0c'),
                    Some('v') => output.push('\x0b'),
                    Some('0') => output.push('\0'),
                    Some(x) if x.is_ascii_digit() => {
                        let mut oct = String::new();
                        oct.push(x);
                        for _ in 0..2 {
                            if let Some(&d) = chars.peek() {
                                if d.is_ascii_digit() {
                                    oct.push(d);
                                    chars.next();
                                }
                            }
                        }
                        if let Ok(n) = u8::from_str_radix(&oct, 8) {
                            output.push(n as char);
                        }
                    }
                    Some(x) => {
                        output.push('\\');
                        output.push(x);
                    }
                    None => output.push('\\'),
                }
            } else {
                output.push(c);
            }
        }
        if !consumed_arg || arg_idx >= fmt_args.len() {
            break;
        }
    }
    print!("{}", output);
    let _ = std::io::Write::flush(&mut std::io::stdout());
    0
}

fn builtin_pwd() -> i32 {
    match std::env::current_dir() {
        Ok(p) => {
            println!("{}", p.display());
            0
        }
        Err(e) => {
            eprintln!("pwd: {}", e);
            1
        }
    }
}

pub fn get_trap_command(sig: &str) -> Option<String> {
    traps().lock().unwrap().get(sig).cloned()
}

fn builtin_command(
    args: &[String],
    vars: &mut ShellVars,
    aliases: &mut HashMap<String, String>,
    funcs: &mut HashMap<String, AstCommand>,
    jobs: &mut JobTable,
    shell_pgid: i32,
) -> i32 {
    let mut i = 1;
    let mut verbose = false;
    let mut default_path = false;

    while i < args.len() {
        match args[i].as_str() {
            "-v" | "-V" => {
                verbose = true;
            }
            "-p" => {
                default_path = true;
            }
            "--" => {
                i += 1;
                break;
            }
            _ if args[i].starts_with('-') && args[i].len() > 1 => {
                eprintln!("command: invalid option: {}", args[i]);
                return 2;
            }
            _ => break,
        }

        i += 1;
    }

    let _ = default_path;

    if i >= args.len() {
        return 0;
    }

    if verbose {
        let mut rc = 0;

        for name in &args[i..] {
            if is_shell_builtin(name) {
                println!("{}", name);
            } else if let Ok(path) = which(name) {
                println!("{}", path);
            } else {
                rc = 1;
            }
        }

        return rc;
    }

    let cmd_name = &args[i];
    let cmd_args = &args[i..];

    if cmd_name != "command" && is_shell_builtin(cmd_name) {
        if let Some(result) =
            run_builtin(cmd_name, cmd_args, vars, aliases, funcs, jobs, shell_pgid)
        {
            return match result {
                ExecResult::Value(v) | ExecResult::Return(v) | ExecResult::Exit(v) => v,
                _ => 0,
            };
        }
    }

    match std::process::Command::new(cmd_name)
        .args(&cmd_args[1..])
        .status()
    {
        Ok(status) => status.code().unwrap_or(127),
        Err(e) => {
            eprintln!("command: {}: {}", cmd_name, e);
            127
        }
    }
}

fn builtin_readonly(args: &[String], vars: &mut ShellVars) -> i32 {
    if args.len() == 1 {
        let mut names: Vec<&String> = vars.readonly.iter().collect();
        names.sort();

        for name in names {
            let val = vars.get(name).unwrap_or_default();
            println!("readonly {}={}", name, shell_quote(&val));
        }

        return 0;
    }

    let mut rc = 0;

    for arg in &args[1..] {
        if let Some((k, v)) = arg.split_once('=') {
            if vars.is_readonly(k) {
                eprintln!("readonly: {}: is read only", k);
                rc = 1;
            } else {
                vars.set_force(k, v, false);
                vars.mark_readonly(k);
            }
        } else if !vars.is_readonly(arg) {
            vars.mark_readonly(arg);
        }
    }

    rc
}

fn builtin_times() -> i32 {
    unsafe {
        let mut tms: libc::tms = std::mem::zeroed();

        if libc::times(&mut tms) < 0 {
            eprintln!("times: {}", std::io::Error::last_os_error());
            return 1;
        }

        let tick = libc::sysconf(libc::_SC_CLK_TCK);
        let tick = if tick <= 0 { 100.0 } else { tick as f64 };

        fn fmt_ticks(t: libc::clock_t, tick: f64) -> String {
            let secs = t as f64 / tick;
            let mins = (secs / 60.0) as i64;
            let rem = secs - (mins as f64) * 60.0;

            format!("{}m{:.3}s", mins, rem)
        }

        println!(
            "{} {}",
            fmt_ticks(tms.tms_utime, tick),
            fmt_ticks(tms.tms_stime, tick)
        );

        println!(
            "{} {}",
            fmt_ticks(tms.tms_cutime, tick),
            fmt_ticks(tms.tms_cstime, tick)
        );
    }

    0
}

fn builtin_getopts(args: &[String], vars: &mut ShellVars) -> i32 {
    if args.len() < 3 {
        return 1;
    }

    let optstring = args[1].clone();
    let varname = args[2].clone();

    let params: Vec<String> = if args.len() > 3 {
        args[3..].to_vec()
    } else {
        vars.positional.clone()
    };

    let mut optind: usize = vars.get("OPTIND").and_then(|s| s.parse().ok()).unwrap_or(1);

    if optind < 1 {
        optind = 1;
    }

    let mut pos: usize = vars
        .get("__SFSH_OPTPOS")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    loop {
        let idx = optind.saturating_sub(1);

        if idx >= params.len() {
            vars.set(&varname, "?", false);
            vars.set("OPTIND", &optind.to_string(), false);
            vars.set("__SFSH_OPTPOS", "1", false);
            return 1;
        }

        let arg = params[idx].clone();

        if pos == 1 {
            if !arg.starts_with('-') || arg == "-" || arg == "--" {
                if arg == "--" {
                    optind += 1;
                    vars.set("OPTIND", &optind.to_string(), false);
                }

                vars.set(&varname, "?", false);
                vars.set("__SFSH_OPTPOS", "1", false);
                return 1;
            }

            if arg.len() <= 1 {
                vars.set(&varname, "?", false);
                vars.set("__SFSH_OPTPOS", "1", false);
                return 1;
            }
        }

        let chars: Vec<char> = arg.chars().collect();

        if pos >= chars.len() {
            optind += 1;
            pos = 1;
            continue;
        }

        let opt = chars[pos];
        let opt_str = opt.to_string();

        let opt_chars: Vec<char> = optstring.chars().collect();
        let mut found = false;
        let mut has_arg = false;

        for (idx2, c) in opt_chars.iter().enumerate() {
            if *c == opt {
                found = true;

                if idx2 + 1 < opt_chars.len() && opt_chars[idx2 + 1] == ':' {
                    has_arg = true;
                }

                break;
            }
        }

        if !found {
            vars.set(&varname, "?", false);

            if optstring.starts_with(':') {
                vars.set("OPTARG", &opt_str, false);
            } else {
                vars.unset("OPTARG");
            }

            if pos + 1 < chars.len() {
                pos += 1;
            } else {
                optind += 1;
                pos = 1;
            }

            vars.set("OPTIND", &optind.to_string(), false);
            vars.set("__SFSH_OPTPOS", &pos.to_string(), false);
            return 0;
        }

        if has_arg {
            let val: String;

            if pos + 1 < chars.len() {
                val = chars[pos + 1..].iter().collect();
                optind += 1;
                pos = 1;
            } else {
                optind += 1;
                pos = 1;

                let next_idx = optind.saturating_sub(1);

                if next_idx < params.len() {
                    val = params[next_idx].clone();
                    optind += 1;
                } else {
                    if optstring.starts_with(':') {
                        vars.set(&varname, ":", false);
                        vars.set("OPTARG", &opt_str, false);
                    } else {
                        vars.set(&varname, "?", false);
                        vars.unset("OPTARG");
                    }

                    vars.set("OPTIND", &optind.to_string(), false);
                    vars.set("__SFSH_OPTPOS", "1", false);
                    return 0;
                }
            }

            vars.set("OPTARG", &val, false);
        } else {
            vars.unset("OPTARG");

            if pos + 1 < chars.len() {
                pos += 1;
            } else {
                optind += 1;
                pos = 1;
            }
        }

        vars.set(&varname, &opt_str, false);
        vars.set("OPTIND", &optind.to_string(), false);
        vars.set("__SFSH_OPTPOS", &pos.to_string(), false);

        return 0;
    }
}

fn builtin_lem(args: &[String]) -> i32 {
    match crate::sfsh::lem::lem_main(&args[1..]) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("lem: {}", e);
            1
        }
    }
}
