use std::env;
use std::process::Command;

fn main() {
    let args: Vec<_> = env::args_os().collect();
    let mut adjustment = 10;
    let mut cmd_idx = 1;

    if args.len() > 2 {
        let s = args[1].to_string_lossy();
        if s == "-n" {
            adjustment = args[2].to_string_lossy().parse().unwrap_or(10);
            cmd_idx = 3;
        }
    }

    if cmd_idx >= args.len() {
        eprintln!("Usage: nice [-n adjustment] <command> [arguments ...]");
        std::process::exit(1);
    }

    let cmd = &args[cmd_idx];
    let cmd_args = &args[cmd_idx + 1..];

    unsafe {
        *libc::__errno_location() = 0;
    }
    let current_prio = unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
    let errno = unsafe { *libc::__errno_location() };
    if current_prio == -1 && errno != 0 {
        eprintln!(
            "nice: cannot get priority: {}",
            std::io::Error::from_raw_os_error(errno)
        );
        std::process::exit(1);
    }

    let new_prio = current_prio + adjustment;

    if unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, new_prio) } < 0 {
        let err = std::io::Error::last_os_error();
        eprintln!("nice: cannot set niceness: {}", err);
    }

    let mut command = Command::new(cmd);
    command.args(cmd_args);

    match command.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(0)),
        Err(e) => {
            eprintln!("nice: failed to run command: {}", e);
            std::process::exit(127);
        }
    }
}
