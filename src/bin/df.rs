use std::env;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::ExitCode;

fn format_size(bytes: u64, human: bool) -> String {
    if !human {
        return format!("{}", bytes / 1024);
    }
    let mut size = bytes as f64;
    let units = ["B", "K", "M", "G", "T", "P"];
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < units.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{}{}", size as u64, units[unit_idx])
    } else {
        let rounded = size.round();
        if rounded.fract() == 0.0 {
            format!("{:.0}{}", rounded, units[unit_idx])
        } else {
            format!("{:.1}{}", size, units[unit_idx])
        }
    }
}

fn get_mount_info(path: &str) -> (String, String) {
    let target_dev = match std::fs::metadata(path) {
        Ok(m) => m.dev(),
        Err(_) => return (path.to_string(), path.to_string()),
    };

    if let Ok(mountinfo) = std::fs::read_to_string("/proc/self/mountinfo") {
        let mut best_mount_point = String::new();
        let mut best_source = String::new();
        let mut best_len = 0;

        for line in mountinfo.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                let mount_point = parts[4];
                let source = parts[9];

                // Выбираем только те точки монтирования, которые являются предками запрошенного пути
                if Path::new(path).starts_with(mount_point) {
                    if let Ok(m) = std::fs::metadata(mount_point) {
                        if m.dev() == target_dev && mount_point.len() > best_len {
                            best_len = mount_point.len();
                            best_mount_point = mount_point.to_string();
                            best_source = source.to_string();
                        }
                    }
                }
            }
        }
        if !best_mount_point.is_empty() {
            return (best_source, best_mount_point);
        }
    }
    (path.to_string(), path.to_string())
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    let mut paths = Vec::new();
    let mut human = false;

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') && s.len() > 1 {
            for c in s.chars().skip(1) {
                match c {
                    'h' => human = true,
                    'k' | 'm' | 'P' => {}
                    _ => {
                        eprintln!("df: invalid option -- '{}'", c);
                        return ExitCode::from(1);
                    }
                }
            }
        } else {
            paths.push(s.into_owned());
        }
    }

    if paths.is_empty() {
        paths.push("/".to_string());
    }

    println!("Filesystem      Size  Used Avail Use% Mounted on");

    for path in paths {
        let mut stats = unsafe { std::mem::zeroed::<libc::statvfs>() };
        let c_path = match std::ffi::CString::new(path.as_bytes()) {
            Ok(c) => c,
            Err(_) => {
                eprintln!("df: invalid path: {}", path);
                continue;
            }
        };

        if unsafe { libc::statvfs(c_path.as_ptr(), &mut stats) } < 0 {
            eprintln!("df: {}: {}", path, io::Error::last_os_error());
            continue;
        }

        let (filesystem, mount_point) = get_mount_info(&path);

        let block_size = stats.f_frsize as u64;
        let total = stats.f_blocks as u64 * block_size;
        let free = stats.f_bfree as u64 * block_size;
        let avail = stats.f_bavail as u64 * block_size;
        let used = total.saturating_sub(free);
        let use_pct = if used + avail > 0 {
            (used * 100) / (used + avail)
        } else {
            0
        };

        println!(
            "{:<15} {:>5} {:>4} {:>5} {:>3}% {}",
            filesystem,
            format_size(total, human),
            format_size(used, human),
            format_size(avail, human),
            use_pct,
            mount_point
        );
    }
    ExitCode::from(0)
}
