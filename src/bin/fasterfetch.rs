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

fn get_shell() -> String {
    if let Ok(shell_path) = env::var("SHELL") {
        if let Ok(resolved) = fs::canonicalize(&shell_path) {
            if let Some(name) = resolved.file_name() {
                return name.to_string_lossy().to_string();
            }
        }
        if let Some(name) = Path::new(&shell_path).file_name() {
            return name.to_string_lossy().to_string();
        }
    }
    let mut pid = std::process::id();
    loop {
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
        if fields.len() < 3 {
            break;
        }
        let ppid: u32 = match fields[1].parse() {
            Ok(p) => p,
            Err(_) => break,
        };
        if ppid <= 1 {
            break;
        }
        let comm_path = format!("/proc/{}/comm", pid);
        if let Ok(comm) = fs::read_to_string(&comm_path) {
            let comm = comm.trim().to_string();
            if is_known_shell(&comm) {
                return comm;
            }
        }
        pid = ppid;
    }
    "sfsh".to_string()
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

    let sqlite_paths = [
        "/var/lib/rpm/rpmdb.sqlite",
        "/usr/lib/sysimage/rpm/rpmdb.sqlite",
        "/var/lib/rpm/rpmdb.sqlite-wal",
    ];
    for path in &sqlite_paths {
        if path.ends_with("-wal") {
            continue;
        }
        if let Some(count) = count_rpm_sqlite(path) {
            if count > 0 {
                return Some(count);
            }
        }
    }
    None
}

fn count_rpm_sqlite(path: &str) -> Option<u32> {
    let data = fs::read(path).ok()?;
    if data.len() < 100 {
        return None;
    }
    if &data[0..15] != b"SQLite format 3" {
        return None;
    }
    let page_size = {
        let raw = u16::from_be_bytes([data[16], data[17]]);
        if raw == 1 {
            65536usize
        } else {
            raw as usize
        }
    };
    let rootpage = sqlite_find_table_root(&data, page_size, "Packages")?;
    Some(sqlite_count_btree(&data, page_size, rootpage as usize))
}

fn sqlite_read_varint(data: &[u8], offset: usize) -> (u64, usize) {
    let mut result: u64 = 0;
    for i in 0..9 {
        if offset + i >= data.len() {
            return (result, i);
        }
        let byte = data[offset + i] as u64;
        if i == 8 {
            result = (result << 8) | byte;
            return (result, 9);
        }
        result = (result << 7) | (byte & 0x7F);
        if byte & 0x80 == 0 {
            return (result, i + 1);
        }
    }
    (result, 9)
}

fn sqlite_page_offset(page_num: usize, page_size: usize) -> usize {
    (page_num - 1) * page_size
}

fn sqlite_btree_hdr(page_num: usize) -> usize {
    if page_num == 1 {
        100
    } else {
        0
    }
}

fn sqlite_find_table_root(data: &[u8], page_size: usize, table_name: &str) -> Option<u32> {
    sqlite_scan_master(data, page_size, 1, table_name)
}

fn sqlite_scan_master(
    data: &[u8],
    page_size: usize,
    page_num: usize,
    table_name: &str,
) -> Option<u32> {
    let page_off = sqlite_page_offset(page_num, page_size);
    let hdr_off = page_off + sqlite_btree_hdr(page_num);
    if hdr_off >= data.len() {
        return None;
    }
    let page_type = data[hdr_off];
    let num_cells = u16::from_be_bytes([data[hdr_off + 3], data[hdr_off + 4]]) as usize;

    match page_type {
        0x0d => {
            let ptr_start = hdr_off + 8;
            for i in 0..num_cells {
                let ptr_off = ptr_start + i * 2;
                if ptr_off + 2 > data.len() {
                    break;
                }
                let cell_off =
                    page_off + u16::from_be_bytes([data[ptr_off], data[ptr_off + 1]]) as usize;
                if let Some(root) = sqlite_parse_master_cell(data, cell_off, table_name) {
                    return Some(root);
                }
            }
        }
        0x05 => {
            let ptr_start = hdr_off + 12;
            for i in 0..num_cells {
                let ptr_off = ptr_start + i * 2;
                if ptr_off + 2 > data.len() {
                    break;
                }
                let cell_off =
                    page_off + u16::from_be_bytes([data[ptr_off], data[ptr_off + 1]]) as usize;
                if cell_off + 4 > data.len() {
                    continue;
                }
                let child = u32::from_be_bytes([
                    data[cell_off],
                    data[cell_off + 1],
                    data[cell_off + 2],
                    data[cell_off + 3],
                ]) as usize;
                if let Some(r) = sqlite_scan_master(data, page_size, child, table_name) {
                    return Some(r);
                }
            }
            let rp_off = hdr_off + 8;
            if rp_off + 4 <= data.len() {
                let right = u32::from_be_bytes([
                    data[rp_off],
                    data[rp_off + 1],
                    data[rp_off + 2],
                    data[rp_off + 3],
                ]) as usize;
                if let Some(r) = sqlite_scan_master(data, page_size, right, table_name) {
                    return Some(r);
                }
            }
        }
        _ => {}
    }
    None
}

fn sqlite_parse_master_cell(data: &[u8], cell_off: usize, table_name: &str) -> Option<u32> {
    let mut off = cell_off;
    if off >= data.len() {
        return None;
    }
    let (_payload_len, n) = sqlite_read_varint(data, off);
    off += n;
    let (_rowid, n) = sqlite_read_varint(data, off);
    off += n;
    if off >= data.len() {
        return None;
    }
    let (header_len, n) = sqlite_read_varint(data, off);
    let header_end = off + header_len as usize;
    let mut serial_off = off + n;
    let mut serial_types = Vec::new();
    while serial_off < header_end && serial_off < data.len() {
        let (st, n) = sqlite_read_varint(data, serial_off);
        serial_types.push(st);
        serial_off += n;
    }
    let mut data_off = header_end;
    let mut values: Vec<Vec<u8>> = Vec::new();
    for st in &serial_types {
        let size = match st {
            0 | 8 | 9 => 0usize,
            1 => 1,
            2 => 2,
            3 => 3,
            4 => 4,
            5 => 6,
            6 => 8,
            7 => 8,
            _ if *st >= 12 && st % 2 == 0 => ((*st - 12) / 2) as usize,
            _ if *st >= 13 && st % 2 == 1 => ((*st - 13) / 2) as usize,
            _ => 0,
        };
        let val = if size > 0 && data_off + size <= data.len() {
            data[data_off..data_off + size].to_vec()
        } else {
            Vec::new()
        };
        values.push(val);
        data_off += size;
    }
    if values.len() >= 4 {
        let name = String::from_utf8_lossy(&values[1]);
        if name == table_name {
            if values[3].len() >= 4 {
                let rp =
                    u32::from_be_bytes([values[3][0], values[3][1], values[3][2], values[3][3]]);
                return Some(rp);
            }
        }
    }
    None
}

fn sqlite_count_btree(data: &[u8], page_size: usize, page_num: usize) -> u32 {
    if page_num == 0 {
        return 0;
    }
    let page_off = sqlite_page_offset(page_num, page_size);
    let hdr_off = page_off + sqlite_btree_hdr(page_num);
    if hdr_off >= data.len() {
        return 0;
    }
    let page_type = data[hdr_off];
    let num_cells = u16::from_be_bytes([data[hdr_off + 3], data[hdr_off + 4]]) as u32;

    match page_type {
        0x0d => num_cells,
        0x05 => {
            let mut count = 0;
            let ptr_start = hdr_off + 12;
            for i in 0..num_cells as usize {
                let ptr_off = ptr_start + i * 2;
                if ptr_off + 2 > data.len() {
                    break;
                }
                let cell_off =
                    page_off + u16::from_be_bytes([data[ptr_off], data[ptr_off + 1]]) as usize;
                if cell_off + 4 > data.len() {
                    continue;
                }
                let child = u32::from_be_bytes([
                    data[cell_off],
                    data[cell_off + 1],
                    data[cell_off + 2],
                    data[cell_off + 3],
                ]) as usize;
                count += sqlite_count_btree(data, page_size, child);
            }
            let rp_off = hdr_off + 8;
            if rp_off + 4 <= data.len() {
                let right = u32::from_be_bytes([
                    data[rp_off],
                    data[rp_off + 1],
                    data[rp_off + 2],
                    data[rp_off + 3],
                ]) as usize;
                count += sqlite_count_btree(data, page_size, right);
            }
            count
        }
        _ => 0,
    }
}

fn get_gpu() -> String {
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

    if let Ok(entries) = fs::read_dir("/sys/bus/pci/devices") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(class_str) = fs::read_to_string(path.join("class")) {
                let class_trimmed = class_str.trim().trim_start_matches("0x");
                if class_trimmed.starts_with("03") {
                    if let (Ok(v_raw), Ok(d_raw)) = (
                        fs::read_to_string(path.join("vendor")),
                        fs::read_to_string(path.join("device")),
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
