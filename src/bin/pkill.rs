use std::env;
use std::fs;

fn parse_signal(arg: &str) -> Option<i32> {
    if arg.starts_with('-') && arg.len() > 1 {
        let sig = &arg[1..];
        if let Ok(num) = sig.parse::<i32>() {
            return Some(num);
        }
        match sig.to_uppercase().as_str() {
            "KILL" | "SIGKILL" => Some(libc::SIGKILL),
            "TERM" | "SIGTERM" => Some(libc::SIGTERM),
            "HUP" | "SIGHUP" => Some(libc::SIGHUP),
            "INT" | "SIGINT" => Some(libc::SIGINT),
            _ => None,
        }
    } else {
        None
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: pkill [-SIGNAL] [-f] <pattern>");
        std::process::exit(1);
    }

    let mut signal = libc::SIGTERM;
    let mut match_full = false;
    let mut patterns = Vec::new();

    for arg in args {
        if arg == "-f" || arg == "--full" {
            match_full = true;
        } else if let Some(sig) = parse_signal(&arg) {
            signal = sig;
        } else {
            patterns.push(arg);
        }
    }

    if patterns.is_empty() {
        eprintln!("pkill: no pattern specified");
        std::process::exit(1);
    }

    let mut matched_any = false;
    let my_pid = unsafe { libc::getpid() };

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.chars().all(|c| c.is_ascii_digit()) {
                let pid = name_str.parse::<i32>().unwrap_or(0);
                if pid == my_pid {
                    continue;
                }

                let proc_path = entry.path();

                let text_to_match = if match_full {
                    // Читаем полную командную строку запуска, заменяя null-байты на пробелы
                    if let Ok(cmdline) = fs::read_to_string(proc_path.join("cmdline")) {
                        cmdline.replace('\0', " ")
                    } else {
                        "".to_string()
                    }
                } else {
                    // Читаем короткое имя исполняемого файла
                    if let Ok(comm) = fs::read_to_string(proc_path.join("comm")) {
                        comm.trim().to_string()
                    } else {
                        "".to_string()
                    }
                };

                if !text_to_match.is_empty() {
                    for pat in &patterns {
                        if text_to_match.contains(pat) {
                            matched_any = true;
                            unsafe {
                                libc::kill(pid, signal);
                            }
                        }
                    }
                }
            }
        }
    }

    if !matched_any {
        std::process::exit(1); // Согласно стандартам, pkill возвращает 1, если ни один процесс не совпал
    }
}
