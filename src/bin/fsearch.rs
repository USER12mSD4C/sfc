use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

fn search_in_file(path: &Path, pattern: &str) {
    if let Ok(file) = File::open(path) {
        let reader = BufReader::new(file);
        for (line_idx, line_res) in reader.lines().enumerate() {
            if let Ok(line) = line_res {
                if let Some(idx) = line.find(pattern) {
                    let before = &line[..idx];
                    let matched = &line[idx..idx + pattern.len()];
                    let after = &line[idx + pattern.len()..];

                    // Выводим имя файла (синим), строку (желтым) и подсвечиваем совпадение (розовым)
                    println!(
                        "\x1b[38;2;137;180;250m{}\x1b[0m:\x1b[38;2;249;226;175m{}\x1b[0m: {}\x1b[1;38;2;243;139;168m{}\x1b[0m{}",
                        path.display(),
                        line_idx + 1,
                        before,
                        matched,
                        after
                    );
                }
            }
        }
    }
}

fn visit_dirs(dir: &Path, pattern: &str) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                // Умный пропуск тяжелых папок для сохранения экстремальной скорости
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
                visit_dirs(&path, pattern);
            } else {
                search_in_file(&path, pattern);
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: fsearch <pattern> [directory]");
        std::process::exit(1);
    }

    let pattern = &args[0];
    let target_dir = args.get(1).map(|s| s.as_str()).unwrap_or(".");
    let path = Path::new(target_dir);

    if !path.exists() {
        eprintln!("fsearch: '{}' does not exist", target_dir);
        std::process::exit(1);
    }

    visit_dirs(path, pattern);
}
