use std::env;
use std::fs;
use std::path::Path;

fn base64_encode_bytes(input: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i] as usize;
        let b1 = if i + 1 < input.len() { input[i + 1] as usize } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] as usize } else { 0 };

        result.push(CHARSET[b0 >> 2] as char);
        result.push(CHARSET[((b0 & 3) << 4) | (b1 >> 4)] as char);
        if i + 1 < input.len() {
            result.push(CHARSET[((b1 & 15) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }
        if i + 2 < input.len() {
            result.push(CHARSET[b2 & 63] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}

fn get_os() -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|content| {
            for line in content.lines() {
                if line.starts_with("PRETTY_NAME=") {
                    return Some(line["PRETTY_NAME=".len()..].trim_matches('"').to_string());
                }
            }
            None
        })
        .unwrap_or_else(|| "Linux".to_string())
}

fn get_kernel() -> String {
    fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "Unknown".to_string())
}

fn get_uptime() -> String {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| {
            let uptime_secs = s.split_whitespace().next()?.parse::<f64>().ok()? as u64;
            let days = uptime_secs / 86400;
            let hours = (uptime_secs % 86400) / 3600;
            let mins = (uptime_secs % 3600) / 60;
            if days > 0 {
                Some(format!("{}d {}h {}m", days, hours, mins))
            } else if hours > 0 {
                Some(format!("{}h {}m", hours, mins))
            } else {
                Some(format!("{}m", mins))
            }
        })
        .unwrap_or_else(|| "Unknown".to_string())
}

fn get_shell() -> String {
    let mut pid = std::process::id();

    if let Ok(stat) = fs::read_to_string(format!("/proc/{}/stat", pid)) {
        if let Some(r_paren) = stat.rfind(')') {
            let fields: Vec<&str> = stat[r_paren + 1..].split_whitespace().collect();
            if fields.len() >= 2 {
                if let Ok(ppid) = fields[1].parse::<u32>() {
                    pid = ppid;
                }
            }
        }
    }

    loop {
        if pid <= 1 {
            break;
        }
        let comm_path = format!("/proc/{}/comm", pid);
        if let Ok(comm) = fs::read_to_string(&comm_path) {
            let comm = comm.trim().to_string();
            if is_known_shell(&comm) {
                return comm;
            }
        }

        let stat_path = format!("/proc/{}/stat", pid);
        let stat = match fs::read_to_string(&stat_path) {
            Ok(s) => s,
            Err(_) => break,
        };
        let r_paren = match stat.rfind(')') {
            Some(p) => p,
            None => break,
        };
        let fields: Vec<&str> = stat[r_paren + 1..].split_whitespace().collect();
        if fields.len() < 2 {
            break;
        }
        let ppid: u32 = match fields[1].parse() {
            Ok(p) => p,
            Err(_) => break,
        };
        pid = ppid;
    }

    if let Ok(shell_path) = env::var("SHELL") {
        if let Some(name) = Path::new(&shell_path).file_name() {
            return name.to_string_lossy().to_string();
        }
    }
    "unknown".to_string()
}

fn is_known_shell(name: &str) -> bool {
    matches!(
        name,
        "sfsh" | "bash" | "zsh" | "fish" | "sh" | "dash" | "ksh" | "tcsh" | "csh" | "ash"
    )
}

fn get_cpu() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|content| {
            for line in content.lines() {
                if line.starts_with("model name") {
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() > 1 {
                        return Some(parts[1].trim().to_string());
                    }
                }
            }
            None
        })
        .unwrap_or_else(|| "Generic CPU".to_string())
}

fn get_mem_swap() -> (String, String) {
    let content = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut mem_total = 0;
    let mut mem_avail = 0;
    let mut swap_total = 0;
    let mut swap_free = 0;

    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or("");
        let val = parts.next().unwrap_or("0").parse::<u64>().unwrap_or(0);
        match key {
            "MemTotal:" => mem_total = val,
            "MemAvailable:" => mem_avail = val,
            "SwapTotal:" => swap_total = val,
            "SwapFree:" => swap_free = val,
            _ => {}
        }
    }

    let mem_used = mem_total.saturating_sub(mem_avail);
    let mem_str = format!(
        "{:.2} GiB / {:.2} GiB ({:.0}%)",
        mem_used as f64 / 1048576.0,
        mem_total as f64 / 1048576.0,
        if mem_total > 0 {
            (mem_used as f64 / mem_total as f64) * 100.0
        } else {
            0.0
        }
    );

    let swap_used = swap_total.saturating_sub(swap_free);
    let swap_str = format!(
        "{:.2} GiB / {:.2} GiB ({:.0}%)",
        swap_used as f64 / 1048576.0,
        swap_total as f64 / 1048576.0,
        if swap_total > 0 {
            (swap_used as f64 / swap_total as f64) * 100.0
        } else {
            0.0
        }
    );

    (mem_str, swap_str)
}

fn get_packages() -> String {
    let mut counts = Vec::new();

    if let Ok(entries) = fs::read_dir("/var/lib/pacman/local") {
        let count = entries.filter(|e| e.is_ok()).count().saturating_sub(1);
        if count > 0 {
            counts.push(format!("{} (pacman)", count));
        }
    }

    if let Ok(content) = fs::read_to_string("/var/lib/dpkg/status") {
        let count = content.matches("Status: install ok installed").count();
        if count > 0 {
            counts.push(format!("{} (dpkg)", count));
        }
    }

    if let Ok(content) = fs::read_to_string("/lib/apk/db/installed") {
        let count = content.matches("\nPackage: ").count();
        if count > 0 {
            counts.push(format!("{} (apk)", count));
        }
    }

    if let Ok(entries) = fs::read_dir("/var/db/xbps") {
        let count = entries.filter(|e| e.is_ok()).count();
        if count > 0 {
            counts.push(format!("{} (xbps)", count));
        }
    }

    if let Some(count) = count_rpm() {
        if count > 0 {
            counts.push(format!("{} (rpm)", count));
        }
    }

    let mut nix_system = 0;
    if let Ok(entries) = fs::read_dir("/run/current-system/sw/bin") {
        nix_system = entries.count();
    }
    let mut nix_user = 0;
    if let Ok(home) = env::var("HOME") {
        let user_path = format!("{}/.nix-profile/bin", home);
        if let Ok(entries) = fs::read_dir(&user_path) {
            nix_user = entries.count();
        }
    }
    if nix_system > 0 || nix_user > 0 {
        if nix_user > 0 {
            counts.push(format!("{} (nix), {} (user)", nix_system, nix_user));
        } else {
            counts.push(format!("{} (nix)", nix_system));
        }
    }

    if counts.is_empty() {
        "Unknown".to_string()
    } else {
        counts.join(", ")
    }
}

fn count_rpm() -> Option<u32> {
    if let Ok(out) = std::process::Command::new("rpm")
        .args(["-qa", "--qf", "x\n"])
        .output()
    {
        if out.status.success() {
            let count = out.stdout.iter().filter(|&&b| b == b'x').count();
            if count > 0 {
                return Some(count as u32);
            }
        }
    }

    let bdb_paths = ["/var/lib/rpm/Packages", "/usr/lib/sysimage/rpm/Packages"];
    for path in &bdb_paths {
        if let Ok(data) = fs::read(path) {
            if data.len() >= 32 {
                let nrecs = u32::from_le_bytes([data[28], data[29], data[30], data[31]]);
                if nrecs > 0 && nrecs < 200_000 {
                    return Some(nrecs);
                }
            }
        }
    }
    None
}

fn get_gpu() -> String {
    if let Ok(entries) = fs::read_dir("/sys/bus/pci/devices") {
        for entry in entries.flatten() {
            let path = entry.path();
            let class_path = path.join("class");
            if let Ok(class_str) = fs::read_to_string(&class_path) {
                let class_trimmed = class_str.trim().trim_start_matches("0x");
                if class_trimmed.starts_with("03") {
                    let vendor_path = path.join("vendor");
                    let device_path = path.join("device");
                    if let (Ok(v_raw), Ok(d_raw)) = (
                        fs::read_to_string(&vendor_path),
                        fs::read_to_string(&device_path),
                    ) {
                        let vendor = v_raw.trim().trim_start_matches("0x").to_uppercase();
                        let device = d_raw.trim().trim_start_matches("0x").to_uppercase();

                        let vendor_name = match vendor.as_str() {
                            "1002" => "AMD Radeon",
                            "10DE" => "NVIDIA",
                            "8086" => "Intel",
                            _ => "Unknown GPU",
                        };

                        if vendor == "1002" {
                            match device.as_str() {
                                "67DF" => return "AMD Radeon RX 570 Series".to_string(),
                                "731F" => return "AMD Radeon RX 5700 Series".to_string(),
                                "743F" => return "AMD Radeon RX 6400/6500 XT".to_string(),
                                "73BF" => return "AMD Radeon RX 6800/6900 Series".to_string(),
                                "744C" => return "AMD Radeon RX 7900 Series".to_string(),
                                _ => return format!("{} (0x{})", vendor_name, device),
                            }
                        } else if vendor == "10DE" {
                            match device.as_str() {
                                "2204" => return "NVIDIA GeForce RTX 3090".to_string(),
                                "2484" => return "NVIDIA GeForce RTX 3070".to_string(),
                                "2684" => return "NVIDIA GeForce RTX 4090".to_string(),
                                "2784" => return "NVIDIA GeForce RTX 4080".to_string(),
                                _ => return format!("{} (0x{})", vendor_name, device),
                            }
                        } else {
                            return format!("{} (0x{})", vendor_name, device);
                        }
                    }
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir("/proc/driver/nvidia/gpus") {
        for entry in entries.flatten() {
            if let Ok(info) = fs::read_to_string(entry.path().join("information")) {
                for line in info.lines() {
                    if line.starts_with("Model:") {
                        return line["Model:".len()..].trim().to_string();
                    }
                }
            }
        }
    }

    "Unknown GPU".to_string()
}

fn get_host() -> String {
    let vendor = fs::read_to_string("/sys/class/dmi/id/sys_vendor")
        .or_else(|_| fs::read_to_string("/sys/devices/virtual/dmi/id/sys_vendor"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let product = fs::read_to_string("/sys/class/dmi/id/product_name")
        .or_else(|_| fs::read_to_string("/sys/devices/virtual/dmi/id/product_name"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let board = fs::read_to_string("/sys/class/dmi/id/board_name")
        .or_else(|_| fs::read_to_string("/sys/devices/virtual/dmi/id/board_name"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    if !vendor.is_empty() && !product.is_empty() {
        format!("{} {}", vendor, product)
    } else if !product.is_empty() {
        product
    } else if !board.is_empty() {
        board
    } else {
        "Generic PC".to_string()
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut image_path = if args.len() > 1 {
        let p = Path::new(&args[1]);
        if p.exists() {
            Some(args[1].clone())
        } else {
            None
        }
    } else {
        let home = env::var("HOME").unwrap_or_default();
        let paths = vec![
            format!("{}/.config/fasterfetch/logo.png", home),
            format!("{}/.config/fasterfetch/logo.jpg", home),
            format!("{}/.config/fasterfetch/logo.jpeg", home),
        ];
        paths.into_iter().find(|p| Path::new(p).exists())
    };

    if let Some(ref path) = image_path {
        if let Ok(abs_path) = fs::canonicalize(path) {
            if let Some(abs_str) = abs_path.to_str() {
                image_path = Some(abs_str.to_string());
            }
        }
    }

    let col_jump = if let Some(ref path) = image_path {
        if let Ok(data) = fs::read(path) {
            let b64 = base64_encode_bytes(&data);
            let chunks: Vec<&str> = b64.as_bytes()
                .chunks(4096)
                .map(|c| std::str::from_utf8(c).unwrap())
                .collect();
            let num_chunks = chunks.len();

            for _ in 0..12 { println!(); }
            print!("\x1b[12A");

            for (i, chunk) in chunks.iter().enumerate() {
                let m = if i == num_chunks - 1 { 0 } else { 1 };
                if i == 0 {
                    print!("\x1b_Ga=T,f=100,t=d,r=12,c=28,m={};{}\x1b\\", m, chunk);
                } else {
                    print!("\x1b_Gm={};{}\x1b\\", m, chunk);
                }
            }
            print!("\x1b[12A");
            "\x1b[32G"
        } else {
            ""
        }
    } else {
        ""
    };

    let username = env::var("USER").unwrap_or_else(|_| "user".to_string());
    let hostname = fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "nixos".to_string());

    let (mem, swap) = get_mem_swap();
    let sep = "-".repeat(username.len() + hostname.len() + 1);

    println!(
        "{}\x1b[1;38;2;166;227;161m{}@{}\x1b[0m",
        col_jump, username, hostname
    );
    println!("{}\x1b[38;2;108;112;134m{}\x1b[0m", col_jump, sep);
    println!(
        "{}\x1b[38;2;203;166;247mOS\x1b[0m:     {}",
        col_jump,
        get_os()
    );
    println!(
        "{}\x1b[38;2;203;166;247mHost\x1b[0m:   {}",
        col_jump,
        get_host()
    );
    println!(
        "{}\x1b[38;2;203;166;247mKernel\x1b[0m: {}",
        col_jump,
        get_kernel()
    );
    println!(
        "{}\x1b[38;2;203;166;247mUptime\x1b[0m: {}",
        col_jump,
        get_uptime()
    );
    println!(
        "{}\x1b[38;2;203;166;247mPkgs\x1b[0m:   {}",
        col_jump,
        get_packages()
    );
    println!(
        "{}\x1b[38;2;203;166;247mShell\x1b[0m:  {}",
        col_jump,
        get_shell()
    );
    println!(
        "{}\x1b[38;2;203;166;247mWM\x1b[0m:     {}",
        col_jump,
        env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "Hyprland".to_string())
    );
    println!(
        "{}\x1b[38;2;203;166;247mCPU\x1b[0m:    {}",
        col_jump,
        get_cpu()
    );
    println!(
        "{}\x1b[38;2;203;166;247mGPU\x1b[0m:    {}",
        col_jump,
        get_gpu()
    );
    println!("{}\x1b[38;2;203;166;247mMemory\x1b[0m: {}", col_jump, mem);
    println!("{}\x1b[38;2;203;166;247mSwap\x1b[0m:   {}", col_jump, swap);

    if image_path.is_some() {
        println!();
    }
}
