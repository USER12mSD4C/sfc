use std::env;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("Usage: stdbuf -i <mode> -o <mode> -e <mode> COMMAND [ARGS...]");
        std::process::exit(1);
    }

    let mut i_mode = None;
    let mut o_mode = None;
    let mut e_mode = None;
    let mut cmd_idx = 0;

    while cmd_idx < args.len() {
        let arg = &args[cmd_idx];
        if arg == "-i" || arg == "--input" {
            i_mode = Some(args.get(cmd_idx + 1).cloned().unwrap_or_default());
            cmd_idx += 2;
        } else if arg == "-o" || arg == "--output" {
            o_mode = Some(args.get(cmd_idx + 1).cloned().unwrap_or_default());
            cmd_idx += 2;
        } else if arg == "-e" || arg == "--error" {
            e_mode = Some(args.get(cmd_idx + 1).cloned().unwrap_or_default());
            cmd_idx += 2;
        } else {
            break;
        }
    }

    if cmd_idx >= args.len() {
        eprintln!("stdbuf: missing command");
        std::process::exit(1);
    }

    let cmd = &args[cmd_idx];
    let cmd_args = &args[cmd_idx + 1..];

    let mut command = Command::new(cmd);
    command.args(cmd_args);

    if let Some(ref m) = i_mode {
        command.env("_STDBUF_I", m);
    }
    if let Some(ref m) = o_mode {
        command.env("_STDBUF_O", m);
    }
    if let Some(ref m) = e_mode {
        command.env("_STDBUF_E", m);
    }

    // Подгружаем стандартную библиотеку буферизации
    command.env("LD_PRELOAD", "libstdbuf.so");

    match command.status() {
        Ok(status) => {
            std::process::exit(status.code().unwrap_or(0));
        }
        Err(e) => {
            eprintln!("stdbuf: failed to execute {}: {}", cmd, e);
            std::process::exit(127);
        }
    }
}
