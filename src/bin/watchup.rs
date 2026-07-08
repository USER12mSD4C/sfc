use std::env;
use std::process::Command;
use std::thread;
use std::time::Duration;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let mut interval = 2;
    let mut cmd_idx = 0;

    // Парсим флаг интервала -n <секунды>
    if args.len() > 2 && (args[0] == "-n" || args[0] == "--interval") {
        if let Ok(val) = args[1].parse::<u64>() {
            interval = val;
            cmd_idx = 2;
        }
    }

    if cmd_idx >= args.len() {
        eprintln!("Usage: watch [-n <seconds>] <command> [args...]");
        std::process::exit(1);
    }

    let cmd = &args[cmd_idx];
    let cmd_args = &args[cmd_idx + 1..];

    loop {
        // Очищаем экран и переносим курсор в верхний левый угол
        print!("\x1b[2J\x1b[H");
        println!(
            "\x1b[1;38;2;203;166;247mEvery {}s:\x1b[0m {} {}\n",
            interval,
            cmd,
            cmd_args.join(" ")
        );

        let mut command = Command::new(cmd);
        command.args(cmd_args);

        if let Err(e) = command.status() {
            eprintln!("watch: failed to execute '{}': {}", cmd, e);
            break;
        }

        thread::sleep(Duration::from_secs(interval));
    }
}
