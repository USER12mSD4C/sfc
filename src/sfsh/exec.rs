use crate::sfsh::ast::{Command as AstCommand, Redirect, Word};
use crate::sfsh::builtin::run_builtin;
use crate::sfsh::expand::{expand_word, match_glob};
use crate::sfsh::job::JobTable;
use crate::sfsh::vars::ShellVars;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, getpgrp, ForkResult, Pid};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::process::CommandExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecResult {
    Value(i32),
    Return(i32),
    Break,
    Continue,
}

pub fn execute_script(
    input: &str,
    vars: &mut ShellVars,
    aliases: &mut HashMap<String, String>,
    funcs: &mut HashMap<String, AstCommand>,
    jobs: &mut JobTable,
    shell_pgid: i32,
) -> i32 {
    let tokens = crate::sfsh::lexer::lex(input);
    let mut parser = crate::sfsh::parser::Parser::new(tokens);
    let ast = parser.parse();
    match execute_command(&ast, vars, aliases, funcs, jobs, shell_pgid, false, None) {
        ExecResult::Value(v) | ExecResult::Return(v) => v,
        ExecResult::Break => {
            eprintln!("sfsh: break: only meaningful in a loop");
            1
        }
        ExecResult::Continue => {
            eprintln!("sfsh: continue: only meaningful in a loop");
            1
        }
    }
}

pub fn execute_script_capture(input: &str, vars: &mut ShellVars) -> Result<String, String> {
    let mut aliases = HashMap::new();
    let mut funcs = HashMap::new();
    let mut jobs = JobTable::new();
    let shell_pgid = getpgrp().as_raw();

    let mut fds = [0i32; 2];
    unsafe {
        libc::pipe(fds.as_mut_ptr());
    }

    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            unsafe {
                libc::close(fds[0]);
                libc::dup2(fds[1], 1);
                libc::close(fds[1]);
                libc::setpgid(0, 0);
            }
            let code = execute_script(input, vars, &mut aliases, &mut funcs, &mut jobs, shell_pgid);
            std::process::exit(code);
        }
        Ok(ForkResult::Parent { child }) => {
            unsafe {
                libc::close(fds[1]);
            }
            let mut output = String::new();
            let mut file = unsafe { File::from_raw_fd(fds[0]) };
            file.read_to_string(&mut output).ok();
            let _ = waitpid(child, None);
            Ok(output)
        }
        Err(e) => Err(e.to_string()),
    }
}

pub fn execute_command(
    cmd: &AstCommand,
    vars: &mut ShellVars,
    aliases: &mut HashMap<String, String>,
    funcs: &mut HashMap<String, AstCommand>,
    jobs: &mut JobTable,
    shell_pgid: i32,
    background: bool,
    pgid: Option<Pid>,
) -> ExecResult {
    match cmd {
        AstCommand::Empty => ExecResult::Value(0),
        AstCommand::Simple(words, redirs) => execute_simple(
            words, redirs, vars, aliases, funcs, jobs, shell_pgid, background, pgid,
        ),
        AstCommand::Pipeline(cmds) => {
            execute_pipeline(cmds, vars, aliases, funcs, jobs, shell_pgid, background)
        }
        AstCommand::List(list) => execute_list(list, vars, aliases, funcs, jobs, shell_pgid),
        AstCommand::If(cond, then_, else_) => {
            let st =
                match execute_command(cond, vars, aliases, funcs, jobs, shell_pgid, false, None) {
                    ExecResult::Value(v) | ExecResult::Return(v) => v,
                    other => return other,
                };
            vars.last_status = st;
            if st == 0 {
                execute_command(then_, vars, aliases, funcs, jobs, shell_pgid, false, None)
            } else if let Some(e) = else_ {
                execute_command(e, vars, aliases, funcs, jobs, shell_pgid, false, None)
            } else {
                ExecResult::Value(0)
            }
        }
        AstCommand::For(var, words, body) => {
            let mut last = 0;
            let expanded: Vec<String> = words.iter().flat_map(|w| expand_word(w, vars)).collect();
            for val in expanded {
                vars.set(var, &val, false);
                match execute_command(body, vars, aliases, funcs, jobs, shell_pgid, false, None) {
                    ExecResult::Break => break,
                    ExecResult::Continue => continue,
                    ExecResult::Return(v) => return ExecResult::Return(v),
                    ExecResult::Value(v) => last = v,
                }
            }
            ExecResult::Value(last)
        }
        AstCommand::While(cond, body) => {
            let mut last = 0;
            loop {
                let st = match execute_command(
                    cond, vars, aliases, funcs, jobs, shell_pgid, false, None,
                ) {
                    ExecResult::Value(v) | ExecResult::Return(v) => v,
                    other => return other,
                };
                if st != 0 {
                    break;
                }
                match execute_command(body, vars, aliases, funcs, jobs, shell_pgid, false, None) {
                    ExecResult::Break => break,
                    ExecResult::Continue => continue,
                    ExecResult::Return(v) => return ExecResult::Return(v),
                    ExecResult::Value(v) => last = v,
                }
            }
            ExecResult::Value(last)
        }
        AstCommand::Until(cond, body) => {
            let mut last = 0;
            loop {
                let st = match execute_command(
                    cond, vars, aliases, funcs, jobs, shell_pgid, false, None,
                ) {
                    ExecResult::Value(v) | ExecResult::Return(v) => v,
                    other => return other,
                };
                if st == 0 {
                    break;
                }
                match execute_command(body, vars, aliases, funcs, jobs, shell_pgid, false, None) {
                    ExecResult::Break => break,
                    ExecResult::Continue => continue,
                    ExecResult::Return(v) => return ExecResult::Return(v),
                    ExecResult::Value(v) => last = v,
                }
            }
            ExecResult::Value(last)
        }
        AstCommand::Case(word, arms) => {
            let val = expand_word(word, vars).join(" ");
            for (pats, cmd) in arms {
                for pat in pats {
                    let p = expand_word(pat, vars).join(" ");
                    if match_glob(&p, &val) {
                        return execute_command(
                            cmd, vars, aliases, funcs, jobs, shell_pgid, false, None,
                        );
                    }
                }
            }
            ExecResult::Value(0)
        }
        AstCommand::Function(name, body) => {
            funcs.insert(name.clone(), *body.clone());
            ExecResult::Value(0)
        }
        AstCommand::Subshell(body) => match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                unsafe {
                    libc::setpgid(0, 0);
                }
                let code = match execute_command(
                    body, vars, aliases, funcs, jobs, shell_pgid, false, None,
                ) {
                    ExecResult::Value(v) | ExecResult::Return(v) => v,
                    _ => 0,
                };
                std::process::exit(code);
            }
            Ok(ForkResult::Parent { child }) => {
                let status = waitpid(child, None).unwrap();
                ExecResult::Value(extract_status(status))
            }
            Err(_) => ExecResult::Value(1),
        },
        AstCommand::Brace(body) => {
            execute_command(body, vars, aliases, funcs, jobs, shell_pgid, false, None)
        }
    }
}

fn execute_simple(
    words: &[Word],
    redirs: &[Redirect],
    vars: &mut ShellVars,
    aliases: &mut HashMap<String, String>,
    funcs: &mut HashMap<String, AstCommand>,
    jobs: &mut JobTable,
    shell_pgid: i32,
    background: bool,
    pgid: Option<Pid>,
) -> ExecResult {
    if words.is_empty() {
        return ExecResult::Value(0);
    }

    let expanded: Vec<String> = words.iter().flat_map(|w| expand_word(w, vars)).collect();
    if expanded.is_empty() {
        return ExecResult::Value(0);
    }

    let first = expanded[0].clone();
    let args = expanded;

    if vars.opts.get(&'x').copied().unwrap_or(false) {
        eprintln!("+ {}", args.join(" "));
    }

    if let Some(func) = funcs.get(&first).cloned() {
        vars.push_local();
        let old_pos = vars.positional.clone();
        vars.set_positional(args[1..].to_vec());
        let result = execute_command(&func, vars, aliases, funcs, jobs, shell_pgid, false, None);
        vars.set_positional(old_pos);
        vars.pop_local();
        return match result {
            ExecResult::Return(v) => ExecResult::Value(v),
            other => other,
        };
    }

    if let Some(result) = run_builtin(&first, &args, vars, aliases, funcs, jobs, shell_pgid) {
        return result;
    }

    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            let my_pgid = pgid.unwrap_or(Pid::from_raw(0));
            unsafe {
                libc::setpgid(0, my_pgid.as_raw());
            }
            if !apply_redirects(redirs, vars) {
                std::process::exit(1);
            }
            unsafe {
                libc::signal(libc::SIGINT, libc::SIG_DFL);
            }
            let err = std::process::Command::new(&first).args(&args[1..]).exec();
            eprintln!("sfsh: {}: {}", first, err);
            std::process::exit(127);
        }
        Ok(ForkResult::Parent { child }) => {
            let child_pgid = Pid::from_raw(child.as_raw());
            let pgid = pgid.unwrap_or(child_pgid);
            unsafe {
                libc::setpgid(child.as_raw(), pgid.as_raw());
            }
            if background {
                let id = jobs.add(pgid, args.join(" "));
                println!("[{}] {}", id, child.as_raw());
                vars.last_bg_pid = child.as_raw() as u32;
                return ExecResult::Value(0);
            }
            if shell_pgid > 0 {
                unsafe {
                    libc::tcsetpgrp(0, pgid.as_raw());
                }
            }
            let mut status = WaitStatus::StillAlive;
            loop {
                match waitpid(child, Some(WaitPidFlag::WUNTRACED)) {
                    Ok(s) => {
                        status = s;
                        break;
                    }
                    Err(nix::errno::Errno::EINTR) => continue,
                    Err(e) => {
                        eprintln!("waitpid: {}", e);
                        break;
                    }
                }
            }
            if shell_pgid > 0 {
                unsafe {
                    libc::tcsetpgrp(0, shell_pgid);
                }
            }
            unsafe {
                libc::signal(libc::SIGINT, libc::SIG_IGN);
            }
            ExecResult::Value(extract_status(status))
        }
        Err(e) => {
            eprintln!("fork: {}", e);
            ExecResult::Value(1)
        }
    }
}

fn execute_pipeline(
    cmds: &[AstCommand],
    vars: &mut ShellVars,
    aliases: &mut HashMap<String, String>,
    funcs: &mut HashMap<String, AstCommand>,
    jobs: &mut JobTable,
    shell_pgid: i32,
    background: bool,
) -> ExecResult {
    if cmds.is_empty() {
        return ExecResult::Value(0);
    }
    if cmds.len() == 1 {
        return execute_command(
            &cmds[0], vars, aliases, funcs, jobs, shell_pgid, background, None,
        );
    }

    let mut pipe_fds = Vec::new();
    for _ in 0..cmds.len() - 1 {
        let mut fds = [0i32; 2];
        unsafe {
            libc::pipe(fds.as_mut_ptr());
        }
        pipe_fds.push((fds[0], fds[1]));
    }

    let mut pgid: Option<Pid> = None;
    let mut children = Vec::new();

    for (i, cmd) in cmds.iter().enumerate() {
        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                if i > 0 {
                    unsafe {
                        libc::dup2(pipe_fds[i - 1].0, 0);
                    }
                }
                if i < cmds.len() - 1 {
                    unsafe {
                        libc::dup2(pipe_fds[i].1, 1);
                    }
                }
                for (r, w) in &pipe_fds {
                    unsafe {
                        libc::close(*r);
                        libc::close(*w);
                    }
                }

                let my_pgid = pgid.unwrap_or(Pid::from_raw(0));
                unsafe {
                    libc::setpgid(0, my_pgid.as_raw());
                }
                unsafe {
                    libc::signal(libc::SIGINT, libc::SIG_DFL);
                }

                if let AstCommand::Simple(words, redirs) = cmd {
                    let expanded: Vec<String> =
                        words.iter().flat_map(|w| expand_word(w, vars)).collect();
                    if expanded.is_empty() {
                        std::process::exit(0);
                    }
                    let first = expanded[0].clone();
                    if !apply_redirects(redirs, vars) {
                        std::process::exit(1);
                    }
                    let err = std::process::Command::new(&first)
                        .args(&expanded[1..])
                        .exec();
                    eprintln!("sfsh: {}: {}", first, err);
                    std::process::exit(127);
                } else {
                    let code = match execute_command(
                        cmd, vars, aliases, funcs, jobs, shell_pgid, false, pgid,
                    ) {
                        ExecResult::Value(v) | ExecResult::Return(v) => v,
                        _ => 0,
                    };
                    std::process::exit(code);
                }
            }
            Ok(ForkResult::Parent { child }) => {
                if pgid.is_none() {
                    pgid = Some(child);
                }
                unsafe {
                    libc::setpgid(child.as_raw(), pgid.unwrap().as_raw());
                }
                children.push(child);
            }
            Err(e) => {
                eprintln!("fork: {}", e);
                for (r, w) in &pipe_fds {
                    unsafe {
                        libc::close(*r);
                        libc::close(*w);
                    }
                }
                return ExecResult::Value(1);
            }
        }
    }

    for (r, w) in &pipe_fds {
        unsafe {
            libc::close(*r);
            libc::close(*w);
        }
    }

    let pgid = pgid.unwrap();

    if background {
        let id = jobs.add(pgid, "pipeline".to_string());
        println!("[{}] {}", id, pgid.as_raw());
        vars.last_bg_pid = pgid.as_raw() as u32;
        return ExecResult::Value(0);
    }

    if shell_pgid > 0 {
        unsafe {
            libc::tcsetpgrp(0, pgid.as_raw());
        }
    }

    let mut last_status = 0;
    for child in children {
        loop {
            match waitpid(child, Some(WaitPidFlag::WUNTRACED)) {
                Ok(status) => {
                    last_status = extract_status(status);
                    break;
                }
                Err(nix::errno::Errno::EINTR) => continue,
                Err(e) => {
                    eprintln!("waitpid: {}", e);
                    break;
                }
            }
        }
    }

    if shell_pgid > 0 {
        unsafe {
            libc::tcsetpgrp(0, shell_pgid);
        }
    }
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
    }
    ExecResult::Value(last_status)
}

fn execute_list(
    list: &[(AstCommand, Option<String>)],
    vars: &mut ShellVars,
    aliases: &mut HashMap<String, String>,
    funcs: &mut HashMap<String, AstCommand>,
    jobs: &mut JobTable,
    shell_pgid: i32,
) -> ExecResult {
    let mut last = 0;
    for (cmd, op) in list {
        let bg = op.as_deref() == Some("&");
        let run = match op.as_deref() {
            Some("&&") => last == 0,
            Some("||") => last != 0,
            _ => true,
        };
        if run {
            let result = execute_command(cmd, vars, aliases, funcs, jobs, shell_pgid, bg, None);
            last = match result {
                ExecResult::Value(v) | ExecResult::Return(v) => v,
                ExecResult::Break | ExecResult::Continue => return result,
            };
            vars.last_status = last;
            if vars.opts.get(&'e').copied().unwrap_or(false) && last != 0 && !bg {
                std::process::exit(last);
            }
        }
    }
    ExecResult::Value(last)
}

fn apply_redirects(redirs: &[Redirect], vars: &mut ShellVars) -> bool {
    for redir in redirs {
        match redir {
            Redirect::In(fd, word) => {
                let path = expand_word(word, vars).join(" ");
                match File::open(&path) {
                    Ok(f) => unsafe {
                        libc::dup2(f.as_raw_fd(), fd.unwrap_or(0) as RawFd);
                    },
                    Err(e) => {
                        eprintln!("sfsh: {}: {}", path, e);
                        return false;
                    }
                }
            }
            Redirect::Out(fd, word) => {
                let path = expand_word(word, vars).join(" ");
                match File::create(&path) {
                    Ok(f) => unsafe {
                        libc::dup2(f.as_raw_fd(), fd.unwrap_or(1) as RawFd);
                    },
                    Err(e) => {
                        eprintln!("sfsh: {}: {}", path, e);
                        return false;
                    }
                }
            }
            Redirect::Append(fd, word) => {
                let path = expand_word(word, vars).join(" ");
                match OpenOptions::new().create(true).append(true).open(&path) {
                    Ok(f) => unsafe {
                        libc::dup2(f.as_raw_fd(), fd.unwrap_or(1) as RawFd);
                    },
                    Err(e) => {
                        eprintln!("sfsh: {}: {}", path, e);
                        return false;
                    }
                }
            }
            Redirect::Here(fd, word, _strip) => {
                let path = expand_word(word, vars).join(" ");
                match File::open(&path) {
                    Ok(f) => unsafe {
                        libc::dup2(f.as_raw_fd(), fd.unwrap_or(0) as RawFd);
                    },
                    Err(e) => {
                        eprintln!("sfsh: heredoc: {}", e);
                        return false;
                    }
                }
            }
            Redirect::DupIn(fd, word) => {
                let s = expand_word(word, vars).join(" ");
                if s == "-" {
                    unsafe {
                        libc::close(fd.unwrap_or(0) as RawFd);
                    }
                } else if let Ok(n) = s.parse::<RawFd>() {
                    unsafe {
                        libc::dup2(n, fd.unwrap_or(0) as RawFd);
                    }
                }
            }
            Redirect::DupOut(fd, word) => {
                let s = expand_word(word, vars).join(" ");
                if s == "-" {
                    unsafe {
                        libc::close(fd.unwrap_or(1) as RawFd);
                    }
                } else if let Ok(n) = s.parse::<RawFd>() {
                    unsafe {
                        libc::dup2(n, fd.unwrap_or(1) as RawFd);
                    }
                }
            }
            Redirect::ReadWrite(fd, word) => {
                let path = expand_word(word, vars).join(" ");
                match OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(&path)
                {
                    Ok(f) => unsafe {
                        libc::dup2(f.as_raw_fd(), fd.unwrap_or(0) as RawFd);
                    },
                    Err(e) => {
                        eprintln!("sfsh: {}: {}", path, e);
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn extract_status(status: WaitStatus) -> i32 {
    match status {
        WaitStatus::Exited(_, code) => code,
        WaitStatus::Signaled(_, sig, _) => 128 + sig as i32,
        _ => 1,
    }
}
