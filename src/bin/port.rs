use std::env;
use std::fs;

fn find_inodes_for_port(target_port: u16) -> Vec<String> {
    let mut inodes = Vec::new();
    // Сканируем таблицы TCP и UDP (как IPv4, так и IPv6)
    let files = [
        "/proc/net/tcp",
        "/proc/net/tcp6",
        "/proc/net/udp",
        "/proc/net/udp6",
    ];

    for file in &files {
        if let Ok(content) = fs::read_to_string(file) {
            for line in content.lines().skip(1) {
                let mut parts = line.split_whitespace();
                let _sl = parts.next();
                let local_addr = parts.next().unwrap_or("");
                let _rem_addr = parts.next();
                let _st = parts.next();
                let _tx_rx = parts.next();
                let _tr_tm = parts.next();
                let _retr = parts.next();
                let _uid = parts.next();
                let _timeout = parts.next();
                let inode = parts.next().unwrap_or("");

                if let Some(port_hex) = local_addr.split(':').nth(1) {
                    if let Ok(port) = u16::from_str_radix(port_hex, 16) {
                        if port == target_port {
                            inodes.push(inode.to_string());
                        }
                    }
                }
            }
        }
    }
    inodes
}

fn find_pids_for_inodes(inodes: &[String]) -> Vec<(u32, String)> {
    let mut results = Vec::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            // Нас интересуют только папки с PID (состоящие из цифр)
            if name_str.chars().all(|c| c.is_ascii_digit()) {
                let pid = name_str.parse::<u32>().unwrap_or(0);
                let fd_dir = entry.path().join("fd");
                if let Ok(fd_entries) = fs::read_dir(fd_dir) {
                    for fd_entry in fd_entries.flatten() {
                        if let Ok(target) = fs::read_link(fd_entry.path()) {
                            let target_str = target.to_string_lossy();
                            for inode in inodes {
                                let socket_pattern = format!("socket:[{}]", inode);
                                if target_str == socket_pattern {
                                    let comm_path = entry.path().join("comm");
                                    let comm = fs::read_to_string(comm_path)
                                        .map(|s| s.trim().to_string())
                                        .unwrap_or_else(|_| "unknown".to_string());
                                    results.push((pid, comm));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    results
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: port <port_number>");
        std::process::exit(1);
    }

    let target_port = match args[0].parse::<u16>() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("port: invalid port number '{}'", args[0]);
            std::process::exit(1);
        }
    };

    let inodes = find_inodes_for_port(target_port);
    if inodes.is_empty() {
        println!("No active socket found on port {}.", target_port);
        return;
    }

    let matches = find_pids_for_inodes(&inodes);
    if matches.is_empty() {
        println!(
            "Socket found on port {}, but couldn't resolve PID (maybe permission denied).",
            target_port
        );
        return;
    }

    // Печатаем красивую компактную таблицу результатов
    println!(
        "\x1b[1;38;2;166;227;161m{: <10} {: <10}\x1b[0m",
        "PID", "COMMAND"
    );
    for (pid, comm) in matches {
        println!("{: <10} {: <10}", pid, comm);
    }
}
