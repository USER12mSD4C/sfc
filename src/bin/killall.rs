use std::env;
use std::fs;
use std::path::Path;

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
        eprintln!("Usage: killall [-SIGNAL] <name> ...");
        std::process::exit(1);
    }

    let mut signal = libc::SIGTERM;
    let mut target_names = Vec::new();

    for arg in args {
        if let Some(sig) = parse_signal(&arg) {
            signal = sig;
        } else {
            target_names.push(arg);
        }
    }

    if target_names.is_empty() {
        eprintln!("killall: no process name specified");
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
                } // Защита от убийства самого себя

                // 1. Получаем усеченное имя (до 15 символов) из comm
                let comm = fs::read_to_string(entry.path().join("comm")).ok();
                let comm_trimmed = comm.as_deref().map(|s| s.trim()).unwrap_or("");

                // 2. Считываем полный cmdline
                let cmdline = fs::read_to_string(entry.path().join("cmdline")).ok();
                let cmdline_args: Vec<String> = cmdline
                    .as_ref()
                    .map(|c| c.split('\0').map(|s| s.to_string()).collect())
                    .unwrap_or_default();

                for target in &target_names {
                    let mut matched = false;

                    // А. Сравниваем с именем из comm (регистронезависимо)
                    if comm_trimmed.to_lowercase() == target.to_lowercase() {
                        matched = true;
                    }

                    // Б. Анализируем cmdline
                    if !matched && !cmdline_args.is_empty() {
                        let first_arg = &cmdline_args[0];
                        if !first_arg.is_empty() {
                            let exe_name = Path::new(first_arg)
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("");

                            // Сравниваем сам исполняемый файл
                            if exe_name.to_lowercase() == target.to_lowercase() {
                                matched = true;
                            } else if exe_name == "python"
                                || exe_name == "python3"
                                || exe_name == "bash"
                                || exe_name == "sh"
                                || exe_name == "node"
                                || exe_name == "appimage-run"
                            {
                                // Если первый аргумент — интерпретатор, проверяем второй аргумент (скрипт/приложение)
                                if cmdline_args.len() > 1 {
                                    let second_arg = &cmdline_args[1];
                                    let script_name = Path::new(second_arg)
                                        .file_name()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("");
                                    if script_name.to_lowercase() == target.to_lowercase() {
                                        matched = true;
                                    }
                                }
                            }
                        }
                    }

                    if matched {
                        matched_any = true;
                        unsafe {
                            if libc::kill(pid, signal) < 0 {
                                let err = std::io::Error::last_os_error();
                                eprintln!("killall: {}({}): {}", target, pid, err);
                            }
                        }
                    }
                }
            }
        }
    }

    if !matched_any {
        eprintln!("killall: no process found");
        std::process::exit(1);
    }
}
