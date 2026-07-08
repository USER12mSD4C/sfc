use std::env;
use std::fs::OpenOptions;
use std::process::{Command, Stdio};

fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        eprintln!("Usage: nohup <command> [arguments ...]");
        std::process::exit(1);
    }

    let cmd = &args[1];
    let cmd_args = &args[2..];

    // Игнорируем SIGHUP на уровне ядра
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }

    let out_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("nohup.out")
        .unwrap();

    let mut child = Command::new(cmd)
        .args(cmd_args)
        .stdout(Stdio::from(out_file))
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("nohup: failed to run command: {}", e);
            std::process::exit(127);
        });

    let _ = child.wait();
}
