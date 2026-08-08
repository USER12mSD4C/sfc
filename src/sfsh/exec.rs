use crate::sfsh::ast::{Command as AstCommand, Redirect, Word};
use crate::sfsh::builtin::run_builtin;
use crate::sfsh::expand::{expand_assignment, expand_word, match_glob};
use crate::sfsh::job::JobTable;
use crate::sfsh::vars::ShellVars;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, getpgrp, ForkResult, Pid};
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::io::{FromRawFd, IntoRawFd, RawFd};
use std::os::unix::process::CommandExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecResult {
    Value(i32),
    Return(i32),
    Break,
    Continue,
    Exit(i32),
}

fn is_interactive() -> bool {
    unsafe { libc::isatty(libc::STDIN_FILENO) != 0 }
}

pub fn shell_exit(code: i32) -> ! {
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let _ = std::io::Write::flush(&mut std::io::stderr());
    std::process::exit(code);
}

struct SavedFd {
    target: RawFd,
    saved: RawFd,
}

fn save_fds(redirs: &[Redirect]) -> Vec<SavedFd> {
    let mut saved = Vec::new();
    let mut targets = HashSet::new();
    for redir in redirs {
        let mut to_save: Vec<RawFd> = Vec::new();
        match redir {
            Redirect::OutErr(_) => {
                to_save.push(1);
                to_save.push(2);
            }
            Redirect::In(fd, _) => to_save.push(fd.unwrap_or(0) as RawFd),
            Redirect::Out(fd, _) => to_save.push(fd.unwrap_or(1) as RawFd),
            Redirect::Append(fd, _) => to_save.push(fd.unwrap_or(1) as RawFd),
            Redirect::Here(fd, _, _, _, _) => to_save.push(fd.unwrap_or(0) as RawFd),
            Redirect::DupIn(fd, _) => to_save.push(fd.unwrap_or(0) as RawFd),
            Redirect::DupOut(fd, _) => to_save.push(fd.unwrap_or(1) as RawFd),
            Redirect::ReadWrite(fd, _) => to_save.push(fd.unwrap_or(0) as RawFd),
        }
        for target in to_save {
            if targets.insert(target) {
                unsafe {
                    let duped = libc::dup(target);
                    if duped >= 0 {
                        saved.push(SavedFd {
                            target,
                            saved: duped,
                        });
                    }
                }
            }
        }
    }
    saved
}

fn restore_fds(saved: &[SavedFd]) {
    for s in saved {
        unsafe {
            libc::dup2(s.saved, s.target);
            libc::close(s.saved);
        }
    }
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

    if let Some(err) = parser.error() {
        eprintln!("sfsh: syntax error: {}", err);
        return 2;
    }

    match execute_command(&ast, vars, aliases, funcs, jobs, shell_pgid, false, None) {
        ExecResult::Value(v) | ExecResult::Return(v) => v,
        ExecResult::Exit(v) => shell_exit(v),
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

pub fn execute_script_capture(
    input: &str,
    vars: &mut ShellVars,
    aliases: &HashMap<String, String>,
    funcs: &HashMap<String, AstCommand>,
) -> Result<String, String> {
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

            let mut child_aliases = unsafe { std::ptr::read(aliases as *const _) };
            let mut child_funcs = unsafe { std::ptr::read(funcs as *const _) };
            let mut child_jobs = JobTable::new();

            let code = execute_script(
                input,
                vars,
                &mut child_aliases,
                &mut child_funcs,
                &mut child_jobs,
                shell_pgid,
            );
            let _ = std::io::Write::flush(&mut std::io::stdout());
            shell_exit(code);
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
        AstCommand::Simple(assignments, words, redirs) => execute_simple(
            assignments,
            words,
            redirs,
            vars,
            aliases,
            funcs,
            jobs,
            shell_pgid,
            background,
            pgid,
        ),
        AstCommand::Pipeline(cmds) => {
            execute_pipeline(cmds, vars, aliases, funcs, jobs, shell_pgid, background)
        }
        AstCommand::List(list) => execute_list(list, vars, aliases, funcs, jobs, shell_pgid),
        AstCommand::If(cond, then_, else_) => {
            let old_e = vars.opts.get(&'e').copied().unwrap_or(false);
            vars.set_opt('e', false);
            let st =
                match execute_command(cond, vars, aliases, funcs, jobs, shell_pgid, false, None) {
                    ExecResult::Value(v) | ExecResult::Return(v) => v,
                    ExecResult::Exit(v) => return ExecResult::Exit(v),
                    other => return other,
                };
            vars.set_opt('e', old_e);
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
            let mut expanded: Vec<String> = Vec::new();
            for w in words {
                expanded.extend(expand_word(w, vars, aliases, funcs));
            }
            let items: Vec<String> = if expanded.is_empty() {
                vars.positional.clone()
            } else {
                expanded
            };
            for val in items {
                vars.set(var, &val, false);
                match execute_command(body, vars, aliases, funcs, jobs, shell_pgid, false, None) {
                    ExecResult::Break => break,
                    ExecResult::Continue => continue,
                    ExecResult::Return(v) => return ExecResult::Return(v),
                    ExecResult::Exit(v) => return ExecResult::Exit(v),
                    ExecResult::Value(v) => last = v,
                }
            }
            ExecResult::Value(last)
        }
        AstCommand::While(cond, body) => {
            let mut last = 0;
            loop {
                let old_e = vars.opts.get(&'e').copied().unwrap_or(false);
                vars.set_opt('e', false);
                let st = match execute_command(
                    cond, vars, aliases, funcs, jobs, shell_pgid, false, None,
                ) {
                    ExecResult::Value(v) | ExecResult::Return(v) => v,
                    ExecResult::Exit(v) => return ExecResult::Exit(v),
                    other => return other,
                };
                vars.set_opt('e', old_e);
                vars.last_status = st;
                if st != 0 {
                    break;
                }
                match execute_command(body, vars, aliases, funcs, jobs, shell_pgid, false, None) {
                    ExecResult::Break => break,
                    ExecResult::Continue => continue,
                    ExecResult::Return(v) => return ExecResult::Return(v),
                    ExecResult::Exit(v) => return ExecResult::Exit(v),
                    ExecResult::Value(v) => last = v,
                }
            }
            ExecResult::Value(last)
        }
        AstCommand::Until(cond, body) => {
            let mut last = 0;
            loop {
                let old_e = vars.opts.get(&'e').copied().unwrap_or(false);
                vars.set_opt('e', false);
                let st = match execute_command(
                    cond, vars, aliases, funcs, jobs, shell_pgid, false, None,
                ) {
                    ExecResult::Value(v) | ExecResult::Return(v) => v,
                    ExecResult::Exit(v) => return ExecResult::Exit(v),
                    other => return other,
                };
                vars.set_opt('e', old_e);
                vars.last_status = st;
                if st == 0 {
                    break;
                }
                match execute_command(body, vars, aliases, funcs, jobs, shell_pgid, false, None) {
                    ExecResult::Break => break,
                    ExecResult::Continue => continue,
                    ExecResult::Return(v) => return ExecResult::Return(v),
                    ExecResult::Exit(v) => return ExecResult::Exit(v),
                    ExecResult::Value(v) => last = v,
                }
            }
            ExecResult::Value(last)
        }
        AstCommand::Case(word, arms) => {
            let val = expand_assignment(word, vars, aliases, funcs);
            for (pats, cmd) in arms {
                for pat in pats {
                    let p = expand_assignment(pat, vars, aliases, funcs);
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
                let code = match execute_command(
                    body, vars, aliases, funcs, jobs, shell_pgid, false, pgid,
                ) {
                    ExecResult::Value(v) | ExecResult::Return(v) | ExecResult::Exit(v) => v,
                    _ => 0,
                };
                if let Some(trap_cmd) = crate::sfsh::builtin::get_trap_command("EXIT") {
                    let _ = execute_script(&trap_cmd, vars, aliases, funcs, jobs, shell_pgid);
                }
                shell_exit(code);
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
        AstCommand::Not(body) => {
            match execute_command(
                body,
                vars,
                aliases,
                funcs,
                jobs,
                shell_pgid,
                background,
                pgid,
            ) {
                ExecResult::Value(0) => ExecResult::Value(1),
                ExecResult::Value(_) => ExecResult::Value(0),
                other => other,
            }
        }
        AstCommand::Cond(expr) => {
            let ok = eval_cond(expr, vars, aliases, funcs);
            ExecResult::Value(if ok { 0 } else { 1 })
        }
    }
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

fn execute_simple(
    assignments: &[(String, Word)],
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
        let saved = save_fds(redirs);
        if !apply_redirects(redirs, vars, aliases, funcs) {
            restore_fds(&saved);
            return ExecResult::Value(1);
        }
        for (name, word) in assignments {
            let val = expand_assignment(word, vars, aliases, funcs);
            vars.set(name, &val, false);
        }
        restore_fds(&saved);
        return ExecResult::Value(0);
    }

    let mut expanded: Vec<String> = Vec::new();
    for w in words {
        expanded.extend(expand_word(w, vars, aliases, funcs));
    }

    if expanded.is_empty() {
        let saved = save_fds(redirs);
        if !apply_redirects(redirs, vars, aliases, funcs) {
            restore_fds(&saved);
            return ExecResult::Value(1);
        }
        restore_fds(&saved);
        return ExecResult::Value(0);
    }

    let mut assignment_values: Vec<(String, String)> = Vec::new();
    for (name, word) in assignments {
        let val = expand_assignment(word, vars, aliases, funcs);
        assignment_values.push((name.clone(), val));
    }

    let mut first = expanded[0].clone();
    let mut args = expanded;

    if let Some(alias_val) = aliases.get(&first) {
        let mut alias_words: Vec<String> = alias_val
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        if !alias_words.is_empty() {
            first = alias_words.remove(0);
            args = std::iter::once(first.clone())
                .chain(alias_words)
                .chain(args.into_iter().skip(1))
                .collect();
        }
    }

    if vars.opts.get(&'x').copied().unwrap_or(false) {
        let mut trace: Vec<String> = assignment_values
            .iter()
            .map(|(name, val)| format!("{}={}", name, val))
            .collect();
        trace.extend(args.iter().cloned());
        eprintln!("+ {}", trace.join(" "));
    }

    let is_builtin = is_shell_builtin(&first);
    let is_func = funcs.contains_key(&first);

    if is_builtin || is_func {
        let saved = save_fds(redirs);
        if !apply_redirects(redirs, vars, aliases, funcs) {
            restore_fds(&saved);
            return ExecResult::Value(1);
        }

        let mut saved_assignments: Vec<(String, Option<String>)> = Vec::new();
        for (name, val) in &assignment_values {
            saved_assignments.push((name.clone(), vars.get(name)));
            vars.set(name, val, false);
        }

        let result = if is_func {
            let func = funcs.get(&first).cloned().unwrap();
            vars.push_local();
            let old_pos = vars.positional.clone();
            vars.set_positional(args[1..].to_vec());
            let result =
                execute_command(&func, vars, aliases, funcs, jobs, shell_pgid, false, None);
            vars.set_positional(old_pos);
            vars.pop_local();
            match result {
                ExecResult::Return(v) => ExecResult::Value(v),
                other => other,
            }
        } else {
            run_builtin(&first, &args, vars, aliases, funcs, jobs, shell_pgid)
                .unwrap_or(ExecResult::Value(0))
        };

        let _ = std::io::Write::flush(&mut std::io::stdout());
        let _ = std::io::Write::flush(&mut std::io::stderr());

        if first == "exec" && args.len() < 2 {
            for s in &saved {
                unsafe {
                    libc::close(s.saved);
                }
            }
            return result;
        }

        for (name, old) in saved_assignments.into_iter().rev() {
            match old {
                Some(v) => vars.set(&name, &v, false),
                None => vars.unset(&name),
            }
        }
        restore_fds(&saved);
        return result;
    }

    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            let my_pgid = pgid.unwrap_or(Pid::from_raw(0));
            unsafe {
                libc::setpgid(0, my_pgid.as_raw());
            }

            if !apply_redirects(redirs, vars, aliases, funcs) {
                shell_exit(1);
            }

            unsafe {
                libc::signal(libc::SIGINT, libc::SIG_DFL);
                libc::signal(libc::SIGQUIT, libc::SIG_DFL);
            }

            let err = {
                let mut cmd = std::process::Command::new(&first);
                cmd.args(&args[1..]);
                for (name, val) in &assignment_values {
                    cmd.env(name, val);
                }
                cmd.exec()
            };

            if let Some(errno) = err.raw_os_error() {
                if errno == libc::ENOEXEC {
                    if let Ok(content) = std::fs::read_to_string(&first) {
                        for (name, val) in &assignment_values {
                            vars.set(name, val, true);
                        }
                        let code = execute_script(&content, vars, aliases, funcs, jobs, shell_pgid);
                        shell_exit(code);
                    }
                }
            }

            eprintln!("sfsh: {}: {}", first, err);
            shell_exit(if err.raw_os_error() == Some(libc::ENOENT) {
                127
            } else {
                126
            });
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

            if shell_pgid > 0 && is_interactive() {
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

            if shell_pgid > 0 && is_interactive() {
                unsafe {
                    libc::tcsetpgrp(0, shell_pgid);
                }
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
                    libc::signal(libc::SIGQUIT, libc::SIG_DFL);
                }

                if let AstCommand::Simple(assignments, words, redirs) = cmd {
                    if words.is_empty() {
                        for (name, word) in assignments {
                            let val = expand_assignment(word, vars, aliases, funcs);
                            vars.set(name, &val, false);
                        }
                        if !apply_redirects(redirs, vars, aliases, funcs) {
                            shell_exit(1);
                        }
                        shell_exit(0);
                    }

                    let mut assignment_values: Vec<(String, String)> = Vec::new();
                    for (name, word) in assignments {
                        let val = expand_assignment(word, vars, aliases, funcs);
                        assignment_values.push((name.clone(), val));
                    }

                    let mut expanded: Vec<String> = Vec::new();
                    for w in words {
                        expanded.extend(expand_word(w, vars, aliases, funcs));
                    }

                    if expanded.is_empty() {
                        shell_exit(0);
                    }

                    let first = expanded[0].clone();

                    if is_shell_builtin(&first) || funcs.contains_key(&first) {
                        if !apply_redirects(redirs, vars, aliases, funcs) {
                            shell_exit(1);
                        }
                        for (name, val) in &assignment_values {
                            vars.set(name, val, false);
                        }
                        let result = if funcs.contains_key(&first) {
                            let func = funcs.get(&first).cloned().unwrap();
                            vars.push_local();
                            vars.set_positional(expanded[1..].to_vec());
                            let r = execute_command(
                                &func, vars, aliases, funcs, jobs, shell_pgid, false, None,
                            );
                            vars.pop_local();
                            r
                        } else {
                            run_builtin(&first, &expanded, vars, aliases, funcs, jobs, shell_pgid)
                                .unwrap_or(ExecResult::Value(0))
                        };
                        let code = match result {
                            ExecResult::Value(v) | ExecResult::Return(v) | ExecResult::Exit(v) => v,
                            _ => 0,
                        };
                        let _ = std::io::Write::flush(&mut std::io::stdout());
                        shell_exit(code);
                    }

                    if !apply_redirects(redirs, vars, aliases, funcs) {
                        shell_exit(1);
                    }

                    let err = {
                        let mut cmd = std::process::Command::new(&first);
                        cmd.args(&expanded[1..]);
                        for (name, val) in &assignment_values {
                            cmd.env(name, val);
                        }
                        cmd.exec()
                    };

                    if let Some(errno) = err.raw_os_error() {
                        if errno == libc::ENOEXEC {
                            if let Ok(content) = std::fs::read_to_string(&first) {
                                for (name, val) in &assignment_values {
                                    vars.set(name, val, true);
                                }
                                let code = execute_script(
                                    &content, vars, aliases, funcs, jobs, shell_pgid,
                                );
                                shell_exit(code);
                            }
                        }
                    }

                    if err.raw_os_error() == Some(libc::ENOENT) {
                        eprintln!("sfsh: {}: command not found", first);
                        shell_exit(127);
                    } else {
                        eprintln!("sfsh: {}: {}", first, err);
                        shell_exit(126);
                    }
                } else {
                    let code = match execute_command(
                        cmd, vars, aliases, funcs, jobs, shell_pgid, false, pgid,
                    ) {
                        ExecResult::Value(v) | ExecResult::Return(v) | ExecResult::Exit(v) => v,
                        _ => 0,
                    };
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                    shell_exit(code);
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

    if shell_pgid > 0 && is_interactive() {
        unsafe {
            libc::tcsetpgrp(0, pgid.as_raw());
        }
    }

    let mut last_status = 0;
    let pipefail = vars.opts.get(&'p').copied().unwrap_or(false);

    for child in children {
        loop {
            match waitpid(child, Some(WaitPidFlag::WUNTRACED)) {
                Ok(status) => {
                    let st = extract_status(status);

                    if !pipefail || st != 0 {
                        last_status = st;
                    }

                    break;
                }
                Err(nix::errno::Errno::EINTR) => continue,
                Err(nix::errno::Errno::ECHILD) => break,
                Err(e) => {
                    eprintln!("waitpid: {}", e);
                    break;
                }
            }
        }
    }

    if shell_pgid > 0 && is_interactive() {
        unsafe {
            libc::tcsetpgrp(0, shell_pgid);
        }
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
    let mut run_current = true;
    for (cmd, op) in list {
        let bg = op.as_deref() == Some("&");
        if run_current {
            let result = execute_command(cmd, vars, aliases, funcs, jobs, shell_pgid, bg, None);
            last = match result {
                ExecResult::Value(v) | ExecResult::Return(v) => v,
                ExecResult::Exit(v) => return ExecResult::Exit(v),
                ExecResult::Break | ExecResult::Continue => return result,
            };
            vars.last_status = last;
            let conditional = matches!(op.as_deref(), Some("&&") | Some("||"));
            if vars.opts.get(&'e').copied().unwrap_or(false) && last != 0 && !bg && !conditional {
                return ExecResult::Exit(last);
            }
        }
        run_current = match op.as_deref() {
            Some("&&") => last == 0,
            Some("||") => last != 0,
            _ => true,
        };
    }
    ExecResult::Value(last)
}

fn dup_fd(raw: RawFd, target: RawFd) -> bool {
    if raw == target {
        return true;
    }
    let res = unsafe { libc::dup2(raw, target) };
    if res < 0 {
        unsafe {
            libc::close(raw);
        }
        eprintln!("sfsh: dup2: {}", std::io::Error::last_os_error());
        return false;
    }
    unsafe {
        libc::close(raw);
    }
    true
}

fn apply_redirects(
    redirs: &[Redirect],
    vars: &mut ShellVars,
    aliases: &HashMap<String, String>,
    funcs: &HashMap<String, AstCommand>,
) -> bool {
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let _ = std::io::Write::flush(&mut std::io::stderr());
    for redir in redirs {
        match redir {
            Redirect::In(fd, word) => {
                let path = expand_word(word, vars, aliases, funcs).join(" ");
                match File::open(&path) {
                    Ok(f) => {
                        let raw = f.into_raw_fd();
                        let target = fd.unwrap_or(0) as RawFd;
                        if !dup_fd(raw, target) {
                            return false;
                        }
                    }
                    Err(e) => {
                        eprintln!("sfsh: {}: {}", path, e);
                        return false;
                    }
                }
            }
            Redirect::Out(fd, word) => {
                let path = expand_word(word, vars, aliases, funcs).join(" ");
                match File::create(&path) {
                    Ok(f) => {
                        let raw = f.into_raw_fd();
                        let target = fd.unwrap_or(1) as RawFd;
                        if !dup_fd(raw, target) {
                            return false;
                        }
                    }
                    Err(e) => {
                        eprintln!("sfsh: {}: {}", path, e);
                        return false;
                    }
                }
            }
            Redirect::Append(fd, word) => {
                let path = expand_word(word, vars, aliases, funcs).join(" ");
                match OpenOptions::new().create(true).append(true).open(&path) {
                    Ok(f) => {
                        let raw = f.into_raw_fd();
                        let target = fd.unwrap_or(1) as RawFd;
                        if !dup_fd(raw, target) {
                            return false;
                        }
                    }
                    Err(e) => {
                        eprintln!("sfsh: {}: {}", path, e);
                        return false;
                    }
                }
            }
            Redirect::Here(fd, _word, _strip, quoted, body) => {
                let body = match body {
                    Some(b) => b.clone(),
                    None => continue,
                };
                let mut expanded = String::new();
                for line in body.lines() {
                    if *quoted {
                        expanded.push_str(line);
                    } else {
                        let word = crate::sfsh::lexer::parse_word(line);
                        let line_expanded = expand_assignment(&word, vars, aliases, funcs);
                        expanded.push_str(&line_expanded);
                    }
                    expanded.push('\n');
                }
                let name = CString::new("sfsh_heredoc").unwrap();
                let raw = unsafe { libc::memfd_create(name.as_ptr(), 0) };
                if raw < 0 {
                    eprintln!("sfsh: memfd_create: {}", std::io::Error::last_os_error());
                    return false;
                }
                let bytes = expanded.as_bytes();
                let mut written = 0;
                while written < bytes.len() {
                    let n = unsafe {
                        libc::write(
                            raw,
                            bytes[written..].as_ptr() as *const libc::c_void,
                            bytes.len() - written,
                        )
                    };
                    if n < 0 {
                        unsafe {
                            libc::close(raw);
                        }
                        eprintln!("sfsh: heredoc write: {}", std::io::Error::last_os_error());
                        return false;
                    }
                    written += n as usize;
                }
                unsafe {
                    libc::lseek(raw, 0, libc::SEEK_SET);
                }
                let target = fd.unwrap_or(0) as RawFd;
                if !dup_fd(raw, target) {
                    return false;
                }
            }
            Redirect::DupIn(fd, word) => {
                let s = expand_word(word, vars, aliases, funcs).join(" ");
                let target = fd.unwrap_or(0) as RawFd;
                if s == "-" {
                    unsafe {
                        libc::close(target);
                    }
                } else if let Ok(n) = s.parse::<RawFd>() {
                    if n != target {
                        let res = unsafe { libc::dup2(n, target) };
                        if res < 0 {
                            eprintln!("sfsh: dup2: {}", std::io::Error::last_os_error());
                            return false;
                        }
                    }
                }
            }
            Redirect::DupOut(fd, word) => {
                let s = expand_word(word, vars, aliases, funcs).join(" ");
                let target = fd.unwrap_or(1) as RawFd;
                if s == "-" {
                    unsafe {
                        libc::close(target);
                    }
                } else if let Ok(n) = s.parse::<RawFd>() {
                    if n != target {
                        let res = unsafe { libc::dup2(n, target) };
                        if res < 0 {
                            eprintln!("sfsh: dup2: {}", std::io::Error::last_os_error());
                            return false;
                        }
                    }
                }
            }
            Redirect::ReadWrite(fd, word) => {
                let path = expand_word(word, vars, aliases, funcs).join(" ");
                match OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(&path)
                {
                    Ok(f) => {
                        let raw = f.into_raw_fd();
                        let target = fd.unwrap_or(0) as RawFd;
                        if !dup_fd(raw, target) {
                            return false;
                        }
                    }
                    Err(e) => {
                        eprintln!("sfsh: {}: {}", path, e);
                        return false;
                    }
                }
            }
            Redirect::OutErr(word) => {
                let path = expand_word(word, vars, aliases, funcs).join(" ");
                match File::create(&path) {
                    Ok(f) => {
                        let raw = f.into_raw_fd();
                        let r1 = unsafe { libc::dup2(raw, 1) };
                        let r2 = unsafe { libc::dup2(raw, 2) };
                        unsafe { libc::close(raw); }
                        if r1 < 0 || r2 < 0 {
                            eprintln!("sfsh: dup2: {}", std::io::Error::last_os_error());
                            return false;
                        }
                    }
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
        WaitStatus::Stopped(_, sig) => 128 + sig as i32,
        WaitStatus::Continued(_) => 0,
        _ => 1,
    }
}

fn eval_cond(
    expr: &crate::sfsh::ast::CondExpr,
    vars: &mut ShellVars,
    aliases: &HashMap<String, String>,
    funcs: &HashMap<String, AstCommand>,
) -> bool {
    use crate::sfsh::ast::CondExpr;

    match expr {
        CondExpr::Or(left, right) => {
            eval_cond(left, vars, aliases, funcs) || eval_cond(right, vars, aliases, funcs)
        }
        CondExpr::And(left, right) => {
            eval_cond(left, vars, aliases, funcs) && eval_cond(right, vars, aliases, funcs)
        }
        CondExpr::Not(inner) => !eval_cond(inner, vars, aliases, funcs),
        CondExpr::Paren(inner) => eval_cond(inner, vars, aliases, funcs),
        CondExpr::Unary(op, word) => {
            let val = expand_assignment(word, vars, aliases, funcs);
            eval_cond_unary(op, &val)
        }
        CondExpr::Binary(op, left, right) => {
            let lhs = expand_assignment(left, vars, aliases, funcs);
            let rhs = expand_assignment(right, vars, aliases, funcs);

            match op.as_str() {
                "<" => lhs < rhs,
                ">" => lhs > rhs,
                "=" | "==" => {
                    if cond_word_is_quoted(right) {
                        lhs == rhs
                    } else {
                        match_glob(&rhs, &lhs)
                    }
                }
                "!=" => {
                    if cond_word_is_quoted(right) {
                        lhs != rhs
                    } else {
                        !match_glob(&rhs, &lhs)
                    }
                }
                "=~" => regex_match(&rhs, &lhs),
                _ => false,
            }
        }
    }
}

fn eval_cond_unary(op: &str, val: &str) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    match op {
        "-z" => val.is_empty(),
        "-n" => !val.is_empty(),
        "-e" => std::path::Path::new(val).exists(),
        "-f" => std::path::Path::new(val).is_file(),
        "-d" => std::path::Path::new(val).is_dir(),
        "-h" | "-L" => std::fs::symlink_metadata(val)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false),
        "-s" => std::fs::metadata(val).map(|m| m.len() > 0).unwrap_or(false),
        "-r" => cond_access(val, libc::R_OK),
        "-w" => cond_access(val, libc::W_OK),
        "-x" => cond_access(val, libc::X_OK),
        "-b" | "-c" | "-p" | "-S" => {
            let mode = std::fs::metadata(val)
                .map(|m| m.permissions().mode())
                .unwrap_or(0);

            match op {
                "-b" => (mode & libc::S_IFMT) == libc::S_IFBLK,
                "-c" => (mode & libc::S_IFMT) == libc::S_IFCHR,
                "-p" => (mode & libc::S_IFMT) == libc::S_IFIFO,
                "-S" => (mode & libc::S_IFMT) == libc::S_IFSOCK,
                _ => false,
            }
        }
        "-t" => val
            .parse::<i32>()
            .map(|fd| unsafe { libc::isatty(fd) != 0 })
            .unwrap_or(false),
        "-u" | "-g" | "-k" => {
            let mode = std::fs::metadata(val)
                .map(|m| m.permissions().mode())
                .unwrap_or(0);

            match op {
                "-u" => mode & libc::S_ISUID != 0,
                "-g" => mode & libc::S_ISGID != 0,
                "-k" => mode & libc::S_ISVTX != 0,
                _ => false,
            }
        }
        "-G" => std::fs::metadata(val)
            .map(|m| m.gid() == unsafe { libc::getegid() })
            .unwrap_or(false),
        "-O" => std::fs::metadata(val)
            .map(|m| m.uid() == unsafe { libc::geteuid() })
            .unwrap_or(false),
        "-N" => std::fs::metadata(val)
            .map(|m| m.mtime() > m.atime())
            .unwrap_or(false),
        _ => false,
    }
}

fn cond_access(path: &str, mode: libc::c_int) -> bool {
    match std::ffi::CString::new(path) {
        Ok(c) => unsafe { libc::access(c.as_ptr(), mode) == 0 },
        Err(_) => false,
    }
}

fn cond_word_is_quoted(w: &Word) -> bool {
    w.0.iter().any(|p| {
        matches!(
            p,
            crate::sfsh::ast::WordPart::SQuote(_) | crate::sfsh::ast::WordPart::DQuote(_)
        )
    })
}

enum RegexKind {
    Char(char),
    Any,
    Class(Vec<(char, char)>, bool),
}

struct RegexToken {
    kind: RegexKind,
    quant: Option<char>,
}

fn regex_match(pattern: &str, text: &str) -> bool {
    let mut p = pattern;
    let mut anchor_start = false;
    let mut anchor_end = false;

    if let Some(stripped) = p.strip_prefix('^') {
        anchor_start = true;
        p = stripped;
    }

    if let Some(stripped) = strip_unescaped_dollar(p) {
        anchor_end = true;
        p = stripped;
    }

    let tokens = match parse_regex_tokens(p) {
        Some(t) => t,
        None => return false,
    };

    let text_chars: Vec<char> = text.chars().collect();

    if anchor_start {
        regex_match_here(&tokens, 0, &text_chars, 0, anchor_end)
    } else {
        for start in 0..=text_chars.len() {
            if regex_match_here(&tokens, 0, &text_chars, start, anchor_end) {
                return true;
            }
        }
        false
    }
}

fn strip_unescaped_dollar(s: &str) -> Option<&str> {
    if !s.ends_with('$') {
        return None;
    }

    let bytes = s.as_bytes();
    let mut backslashes = 0;
    let mut i = bytes.len() as isize - 2;

    while i >= 0 && bytes[i as usize] == b'\\' {
        backslashes += 1;
        i -= 1;
    }

    if backslashes % 2 == 0 {
        Some(&s[..s.len() - 1])
    } else {
        None
    }
}

fn parse_regex_tokens(p: &str) -> Option<Vec<RegexToken>> {
    let chars: Vec<char> = p.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let kind = match chars[i] {
            '\\' => {
                i += 1;
                if i >= chars.len() {
                    return None;
                }
                let c = chars[i];
                i += 1;
                RegexKind::Char(c)
            }
            '.' => {
                i += 1;
                RegexKind::Any
            }
            '[' => {
                let (kind, next) = parse_regex_class(&chars, i + 1)?;
                i = next;
                kind
            }
            c => {
                i += 1;
                RegexKind::Char(c)
            }
        };

        let quant = if i < chars.len() && (chars[i] == '*' || chars[i] == '+' || chars[i] == '?') {
            let q = chars[i];
            i += 1;
            Some(q)
        } else {
            None
        };

        tokens.push(RegexToken { kind, quant });
    }

    Some(tokens)
}

fn parse_regex_class(chars: &[char], mut i: usize) -> Option<(RegexKind, usize)> {
    let mut negate = false;

    if i < chars.len() && (chars[i] == '!' || chars[i] == '^') {
        negate = true;
        i += 1;
    }

    let mut ranges = Vec::new();

    if i < chars.len() && chars[i] == ']' {
        ranges.push((']', ']'));
        i += 1;
    }

    while i < chars.len() && chars[i] != ']' {
        let start = if chars[i] == '\\' {
            i += 1;
            if i >= chars.len() {
                return None;
            }
            chars[i]
        } else {
            chars[i]
        };

        i += 1;

        if i + 1 < chars.len() && chars[i] == '-' && chars[i + 1] != ']' {
            i += 1;

            let end = if chars[i] == '\\' {
                i += 1;
                if i >= chars.len() {
                    return None;
                }
                chars[i]
            } else {
                chars[i]
            };

            i += 1;
            ranges.push((start, end));
        } else {
            ranges.push((start, start));
        }
    }

    if i >= chars.len() {
        return None;
    }

    i += 1;

    Some((RegexKind::Class(ranges, negate), i))
}

fn regex_match_here(
    tokens: &[RegexToken],
    ti: usize,
    text: &[char],
    si: usize,
    anchor_end: bool,
) -> bool {
    if ti == tokens.len() {
        return if anchor_end { si == text.len() } else { true };
    }

    let token = &tokens[ti];

    match token.quant {
        Some('*') => {
            if regex_match_here(tokens, ti + 1, text, si, anchor_end) {
                return true;
            }

            if si < text.len() && regex_match_one(&token.kind, text[si]) {
                return regex_match_here(tokens, ti, text, si + 1, anchor_end);
            }

            false
        }
        Some('+') => {
            if si < text.len() && regex_match_one(&token.kind, text[si]) {
                if regex_match_here(tokens, ti + 1, text, si + 1, anchor_end) {
                    return true;
                }

                return regex_match_here(tokens, ti, text, si + 1, anchor_end);
            }

            false
        }
        Some('?') => {
            if regex_match_here(tokens, ti + 1, text, si, anchor_end) {
                return true;
            }

            if si < text.len() && regex_match_one(&token.kind, text[si]) {
                return regex_match_here(tokens, ti + 1, text, si + 1, anchor_end);
            }

            false
        }
        _ => {
            si < text.len()
                && regex_match_one(&token.kind, text[si])
                && regex_match_here(tokens, ti + 1, text, si + 1, anchor_end)
        }
    }
}

fn regex_match_one(kind: &RegexKind, c: char) -> bool {
    match kind {
        RegexKind::Char(x) => c == *x,
        RegexKind::Any => true,
        RegexKind::Class(ranges, negate) => {
            let mut ok = false;

            for (a, b) in ranges {
                if c >= *a && c <= *b {
                    ok = true;
                    break;
                }
            }

            if *negate {
                !ok
            } else {
                ok
            }
        }
    }
}
