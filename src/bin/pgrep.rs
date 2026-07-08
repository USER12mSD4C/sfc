use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let mut match_full = false;
    let mut list_name = false;
    let mut invert = false;
    let mut pattern = None;

    for arg in args {
        if arg.starts_with('-') && arg.len() > 1 {
            for c in arg.chars().skip(1) {
                match c {
                    'f' => match_full = true,
                    'l' => list_name = true,
                    'v' => invert = true,
                    _ => {
                        eprintln!("pgrep: invalid option -- '{}'", c);
                        std::process::exit(2);
                    }
                }
            }
        } else if pattern.is_none() {
            pattern = Some(arg);
        } else {
            eprintln!("pgrep: only one pattern can be provided");
            std::process::exit(2);
        }
    }

    let pattern = match pattern {
        Some(p) => p,
        None => {
            eprintln!("Usage: pgrep [-flv] <pattern>");
            std::process::exit(2);
        }
    };

    let my_pid = unsafe { libc::getpid() };
    let mut matched_any = false;

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.chars().all(|c| c.is_ascii_digit()) {
                let pid = name_str.parse::<i32>().unwrap_or(0);
                if pid == my_pid {
                    continue;
                }

                let comm = fs::read_to_string(entry.path().join("comm"))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();

                let cmdline = fs::read_to_string(entry.path().join("cmdline"))
                    .map(|s| s.replace('\0', " ").trim().to_string())
                    .unwrap_or_default();

                let match_target = if match_full { &cmdline } else { &comm };

                let is_match = match_target.contains(&pattern);
                let should_print = if invert { !is_match } else { is_match };

                if should_print && !match_target.is_empty() {
                    matched_any = true;
                    if list_name {
                        println!("{} {}", pid, comm);
                    } else {
                        println!("{}", pid);
                    }
                }
            }
        }
    }

    if matched_any {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}
