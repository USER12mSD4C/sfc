use std::env;
use std::process::Command;

fn main() {
    let args: Vec<_> = env::args_os().collect();
    let mut clear_env = false;
    let mut args_iter = args.iter().skip(1).peekable();

    while let Some(&arg) = args_iter.peek() {
        let s = arg.to_string_lossy();
        if s == "-i" || s == "--ignore-environment" {
            clear_env = true;
            args_iter.next();
        } else if s.starts_with('-') {
            eprintln!("env: invalid option: {}", s);
            std::process::exit(1);
        } else {
            break;
        }
    }

    if clear_env {
        for (key, _) in env::vars_os() {
            env::remove_var(key);
        }
    }

    while let Some(&arg) = args_iter.peek() {
        let s = arg.to_string_lossy();
        if s.contains('=') {
            let parts: Vec<&str> = s.splitn(2, '=').collect();
            env::set_var(parts[0], parts[1]);
            args_iter.next();
        } else {
            break;
        }
    }

    if args_iter.peek().is_none() {
        let mut stdout = std::io::stdout().lock();
        for (key, val) in env::vars_os() {
            let _ = std::io::Write::write_all(&mut stdout, key.as_encoded_bytes());
            let _ = std::io::Write::write_all(&mut stdout, b"=");
            let _ = std::io::Write::write_all(&mut stdout, val.as_encoded_bytes());
            let _ = std::io::Write::write_all(&mut stdout, b"\n");
        }
        return;
    }

    let cmd = args_iter.next().unwrap();
    let cmd_args: Vec<_> = args_iter.collect();

    let mut command = Command::new(cmd);
    command.args(cmd_args);

    match command.status() {
        Ok(status) => {
            std::process::exit(status.code().unwrap_or(0));
        }
        Err(e) => {
            eprintln!("env: {}: {}", cmd.to_string_lossy(), e);
            std::process::exit(127);
        }
    }
}
