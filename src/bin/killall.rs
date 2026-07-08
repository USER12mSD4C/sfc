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
                } // Защита от убийства самого себя (процесса killall)

                // 1. Получаем усеченное имя (до 15 символов) из comm
                let comm = fs::read_to_string(entry.path().join("comm")).ok();
                let comm_trimmed = comm.as_deref().map(|s| s.trim()).unwrap_or("");

                // 2. Получаем полное имя исполняемого файла из cmdline (чтобы избежать лимита в 15 символов)
                let cmdline = fs::read_to_string(entry.path().join("cmdline")).ok();
                let cmdline_name = cmdline
                    .as_ref()
                    .and_then(|c| {
                        c.split('\0').next().and_then(|path_str| {
                            if path_str.is_empty() {
                                None
                            } else {
                                Path::new(path_str).file_name().and_then(|s| s.to_str())
                            }
                        })
                    })
                    .unwrap_or("");

                for target in &target_names {
                    let matches_comm = comm_trimmed == target;
                    let matches_cmdline = !cmdline_name.is_empty() && cmdline_name == target;

                    // Если совпало по любому из методов — убиваем
                    if matches_comm || matches_cmdline {
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
