// src/bin/nice.rs
use std::env;
use std::process::Command;
use std::process::ExitCode;

const VERSION: &str = "nice (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: nice [OPTION] [COMMAND [ARG]...]\n\
Run COMMAND with an adjusted niceness, which affects process scheduling.\n\
With no COMMAND, print the current niceness.  Niceness values range from\n\
-20 (most favorable to the process) to 19 (least favorable to the process).\n\
\n\
  -n, --adjustment=N   add integer N to the niceness (default 10)\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

struct Options {
    adjustment: i32,
    command: Option<String>,
    args: Vec<String>,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        adjustment: 10,
        command: None,
        args: Vec::new(),
    };

    let mut args = env::args().skip(1);
    let mut end_of_opts = false;

    while let Some(arg) = args.next() {
        if !end_of_opts && arg.starts_with('-') && arg.len() > 1 {
            if arg == "--" {
                end_of_opts = true;
                continue;
            }
            if arg == "--help" {
                println!("{}", HELP);
                std::process::exit(0);
            }
            if arg == "--version" {
                println!("{}", VERSION);
                std::process::exit(0);
            }

            if arg.starts_with("--adjustment=") {
                opts.adjustment = arg[13..].parse().map_err(|_| "invalid adjustment")?;
                continue;
            }
            if arg.starts_with("-n=") {
                opts.adjustment = arg[3..].parse().map_err(|_| "invalid adjustment")?;
                continue;
            }
            if arg == "-n" {
                let val = args.next().ok_or("-n requires argument")?;
                opts.adjustment = val.parse().map_err(|_| "invalid adjustment")?;
                continue;
            }

            for c in arg.chars().skip(1) {
                match c {
                    'n' => {
                        let val = args.next().ok_or("-n requires argument")?;
                        opts.adjustment = val.parse().map_err(|_| "invalid adjustment")?;
                        break;
                    }
                    _ => return Err(format!("invalid option -- '{}'", c)),
                }
            }
        } else {
            opts.command = Some(arg);
            opts.args = args.collect();
            break;
        }
    }

    Ok(opts)
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("nice: {}", e);
            eprintln!("Try 'nice --help' for more information.");
            return ExitCode::from(1);
        }
    };

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
        return ExitCode::from(1);
    }

    if let Some(cmd) = opts.command {
        let new_prio = current_prio + opts.adjustment;
        if unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, new_prio) } < 0 {
            let err = std::io::Error::last_os_error();
            eprintln!("nice: cannot set niceness: {}", err);
        }

        let mut command = Command::new(&cmd);
        command.args(&opts.args);

        match command.status() {
            Ok(status) => ExitCode::from(status.code().unwrap_or(0) as u8),
            Err(e) => {
                eprintln!("nice: failed to run command: {}", e);
                ExitCode::from(127)
            }
        }
    } else {
        println!("{}", current_prio);
        ExitCode::from(0)
    }
}
