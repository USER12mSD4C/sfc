use std::env;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

struct Stats {
    lines: usize,
    words: usize,
    bytes: usize,
}

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut count_lines = false;
    let mut count_words = false;
    let mut count_bytes = false;
    let mut paths = Vec::new();

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') && s != "-" {
            for c in s.chars().skip(1) {
                match c {
                    'l' => count_lines = true,
                    'w' => count_words = true,
                    'c' => count_bytes = true,
                    _ => {
                        eprintln!("wc: invalid option: -{}", c);
                        std::process::exit(1);
                    }
                }
            }
        } else {
            paths.push(Path::new(arg));
        }
    }

    if !count_lines && !count_words && !count_bytes {
        count_lines = true;
        count_words = true;
        count_bytes = true;
    }

    // Рассчитываем динамическую ширину колонок по алгоритму GNU wc:
    // Ширина равна количеству цифр в сумме размеров всех файлов на входе.
    // Если файлов нет (stdin), GNU wc по умолчанию использует ширину 7.
    let mut total_size = 0;
    for path in &paths {
        if let Ok(meta) = fs::metadata(path) {
            total_size += meta.len();
        }
    }
    let mut width = total_size.to_string().len();
    if paths.is_empty() {
        width = 7;
    }

    if paths.is_empty() {
        let stdin = io::stdin();
        let stats = process_reader(stdin.lock())?;
        print_stats(&stats, "", count_lines, count_words, count_bytes, 0, width);
    } else {
        let mut total_lines = 0;
        let mut total_words = 0;
        let mut total_bytes = 0;

        let paths_len = paths.len();

        for path in &paths {
            let file = match File::open(path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("wc: {}: {}", path.to_string_lossy(), e);
                    continue;
                }
            };
            let stats = process_reader(file)?;
            print_stats(
                &stats,
                &path.to_string_lossy(),
                count_lines,
                count_words,
                count_bytes,
                paths_len,
                width,
            );
            total_lines += stats.lines;
            total_words += stats.words;
            total_bytes += stats.bytes;
        }

        if paths_len > 1 {
            let total = Stats {
                lines: total_lines,
                words: total_words,
                bytes: total_bytes,
            };
            print_stats(
                &total,
                "total",
                count_lines,
                count_words,
                count_bytes,
                paths_len,
                width,
            );
        }
    }

    Ok(())
}

fn process_reader<R: Read>(mut reader: R) -> io::Result<Stats> {
    let mut lines = 0;
    let mut words = 0;
    let mut bytes = 0;
    let mut in_word = false;

    let mut buffer = [0u8; 16384];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n;

        for &b in &buffer[..n] {
            if b == b'\n' {
                lines += 1;
            }
            if b.is_ascii_whitespace() {
                in_word = false;
            } else if !in_word {
                in_word = true;
                words += 1;
            }
        }
    }

    Ok(Stats {
        lines,
        words,
        bytes,
    })
}

fn print_stats(
    stats: &Stats,
    name: &str,
    count_lines: bool,
    count_words: bool,
    count_bytes: bool,
    paths_len: usize,
    width: usize,
) {
    let active_stats = count_lines as usize + count_words as usize + count_bytes as usize;
    let pad = !(paths_len <= 1 && active_stats == 1);

    let mut parts = Vec::new();
    if count_lines {
        if pad {
            parts.push(format!("{:>width$}", stats.lines, width = width));
        } else {
            parts.push(stats.lines.to_string());
        }
    }
    if count_words {
        if pad {
            parts.push(format!("{:>width$}", stats.words, width = width));
        } else {
            parts.push(stats.words.to_string());
        }
    }
    if count_bytes {
        if pad {
            parts.push(format!("{:>width$}", stats.bytes, width = width));
        } else {
            parts.push(stats.bytes.to_string());
        }
    }
    if !name.is_empty() {
        parts.push(name.to_string());
    }
    println!("{}", parts.join(" "));
}
