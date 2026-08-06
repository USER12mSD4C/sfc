use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::ExitCode;

fn get_dir_size(path: &Path) -> io::Result<u64> {
    let mut total = 0;
    let metadata = fs::symlink_metadata(path)?;
    total += metadata.blocks() * 512;

    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            total += get_dir_size(&entry.path()).unwrap_or(0);
        }
    }
    Ok(total)
}

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
        format!("{:.0}{}", size, units[unit_idx])
    }
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    let mut targets = Vec::new();
    let mut summary = false;
    let mut human = false;

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') && s.len() > 1 {
            for c in s.chars().skip(1) {
                match c {
                    's' => summary = true,
                    'h' => human = true,
                    _ => {}
                }
            }
        } else {
            targets.push(s.into_owned());
        }
    }

    if targets.is_empty() {
        targets.push(".".to_string());
    }

    for target in targets {
        let path = Path::new(&target);

        if summary || !path.is_dir() {
            let size = if path.is_dir() {
                get_dir_size(path).unwrap_or(0)
            } else {
                fs::metadata(path).map(|m| m.blocks() * 512).unwrap_or(0)
            };
            println!("{}\t{}", format_size(size, human), target);
        } else {
            for entry in
                fs::read_dir(path).unwrap_or_else(|_| panic!("cannot read directory: {}", target))
            {
                if let Ok(entry) = entry {
                    let p = entry.path();
                    let size = get_dir_size(&p).unwrap_or(0);
                    println!("{}\t{}", format_size(size, human), p.to_string_lossy());
                }
            }
        }
    }
    ExitCode::from(0)
}
