use std::env;
use std::fs;
use std::path::Path;

fn get_dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += get_dir_size(&p);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let target_dir = args.get(0).map(|s| s.as_str()).unwrap_or(".");
    let path = Path::new(target_dir);

    if !path.is_dir() {
        eprintln!("fsize: '{}' is not a directory", target_dir);
        std::process::exit(1);
    }

    println!("Analyzing elements in '{}'...", path.display());

    let mut items = Vec::new();

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let size = if p.is_dir() {
                get_dir_size(&p)
            } else {
                p.metadata().map(|m| m.len()).unwrap_or(0)
            };
            items.push((name, size, p.is_dir()));
        }
    }

    // Сортируем элементы строго по размеру (по убыванию)
    items.sort_by(|a, b| b.1.cmp(&a.1));

    println!(
        "\n\x1b[1;38;2;203;166;247m{: <15} {: <10} {}\x1b[0m",
        "SIZE", "TYPE", "NAME"
    );
    for (name, size, is_dir) in items {
        let size_str = format_size(size);
        let type_str = if is_dir {
            "\x1b[38;2;137;180;250mDIR\x1b[0m"
        } else {
            "\x1b[38;2;166;227;161mFILE\x1b[0m"
        };
        println!("{: <15} {: <10} {}", size_str, type_str, name);
    }
}
