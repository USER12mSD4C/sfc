use std::env;
use std::os::unix::process::ExitStatusExt;
use std::process::Command;
use std::thread;
use std::time::Duration;

fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 3 {
        eprintln!("Usage: timeout <seconds> <command> [arguments ...]");
        std::process::exit(1);
    }

    let sec_str = args[1].to_string_lossy();
    let secs = match sec_str.parse::<u64>() {
        Ok(s) => s,
        Err(_) => {
            eprintln!("timeout: invalid duration: {}", sec_str);
            std::process::exit(1);
        }
    };

    let cmd = &args[2];
    let cmd_args = &args[3..];

    let mut child = match Command::new(cmd).args(cmd_args).spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("timeout: failed to run command: {}", e);
            std::process::exit(127);
        }
    };

    let child_id = child.id();

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(secs));
        unsafe {
            libc::kill(child_id as libc::pid_t, libc::SIGTERM);
        }
    });

    match child.wait() {
        Ok(status) => {
            if let Some(code) = status.code() {
                std::process::exit(code);
            } else if let Some(signal) = status.signal() {
                if signal == libc::SIGTERM {
                    std::process::exit(124);
                } else {
                    std::process::exit(128 + signal);
                }
            } else {
                std::process::exit(1);
            }
        }
        Err(_) => std::process::exit(1),
    }
}
