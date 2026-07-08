use std::env;
use std::fs;
use std::path::Path;

fn base64_encode(input: &str) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i] as usize;
        let b1 = if i + 1 < bytes.len() {
            bytes[i + 1] as usize
        } else {
            0
        };
        let b2 = if i + 2 < bytes.len() {
            bytes[i + 2] as usize
        } else {
            0
        };

        let c0 = b0 >> 2;
        let c1 = ((b0 & 3) << 4) | (b1 >> 4);
        let c2 = ((b1 & 15) << 2) | (b2 >> 6);
        let c3 = b2 & 63;

        result.push(CHARSET[c0] as char);
        result.push(CHARSET[c1] as char);
        if i + 1 < bytes.len() {
            result.push(CHARSET[c2] as char);
        } else {
            result.push('=');
        }
        if i + 2 < bytes.len() {
            result.push(CHARSET[c3] as char);
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

fn get_parent_pid() -> Option<u32> {
    let stat = fs::read_to_string("/proc/self/stat").ok()?;
    let r_paren = stat.rfind(')')?;
    let after_paren = &stat[r_paren + 1..];
    let mut fields = after_paren.split_whitespace();
    let _state = fields.next()?;
    let ppid_str = fields.next()?;
    ppid_str.parse::<u32>().ok()
}

fn get_shell() -> String {
    if let Some(ppid) = get_parent_pid() {
        let comm_path = format!("/proc/{}/comm", ppid);
        if let Ok(comm) = fs::read_to_string(comm_path) {
            return comm.trim().to_string();
        }
    }
    env::var("SHELL")
        .ok()
        .and_then(|s| s.split('/').last().map(|s| s.to_string()))
        .unwrap_or_else(|| "sfshell".to_string())
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
    let mut system_pkgs = 0;
    if let Ok(entries) = fs::read_dir("/run/current-system/sw/bin") {
        system_pkgs = entries.count();
    }
    let mut user_pkgs = 0;
    if let Ok(home) = env::var("HOME") {
        let user_path = format!("{}/.nix-profile/bin", home);
        if let Ok(entries) = fs::read_dir(user_path) {
            user_pkgs = entries.count();
        }
    }
    if user_pkgs > 0 {
        format!("{} (nix-system), {} (nix-user)", system_pkgs, user_pkgs)
    } else {
        format!("{} (nix-system)", system_pkgs)
    }
}

fn get_gpu() -> String {
    // 1. Попытка для NVIDIA с проприетарными драйверами
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

    // 2. Универсальный обход шины PCI для детекции любых карт (AMD, Intel, NVIDIA в драйвере Nouveau)
    if let Ok(entries) = fs::read_dir("/sys/bus/pci/devices") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(class_str) = fs::read_to_string(path.join("class")) {
                let class_trimmed = class_str.trim().trim_start_matches("0x");
                // Класс PCI 03xxxx означает дисплейный адаптер (видеокарту)
                if class_trimmed.starts_with("03") {
                    if let (Ok(v_raw), Ok(d_raw)) = (
                        fs::read_to_string(path.join("vendor")),
                        fs::read_to_string(path.join("device")),
                    ) {
                        let vendor = v_raw.trim().trim_start_matches("0x").to_uppercase();
                        let device = d_raw.trim().trim_start_matches("0x").to_uppercase();

                        // Сопоставляем вендоров
                        let vendor_name = match vendor.as_str() {
                            "1002" => "AMD Radeon",
                            "10DE" => "NVIDIA",
                            "8086" => "Intel",
                            _ => "Unknown GPU",
                        };

                        // Таблица популярных моделей
                        if vendor == "1002" {
                            match device.as_str() {
                                "67DF" => return "AMD Radeon RX 570 Series".to_string(), // Ваша RX 570!
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
        let b64_path = base64_encode(path);

        for _ in 0..12 {
            println!();
        }
        print!("\x1b[12A");

        print!("\x1b_Ga=T,f=100,t=f,r=12,c=28;{}\x1b\\", b64_path);

        print!("\x1b[12A");

        "\x1b[32G"
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
