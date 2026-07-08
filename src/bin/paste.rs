use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut paths = Vec::new();

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') {
            eprintln!("paste: invalid option: {}", s);
            std::process::exit(1);
        } else {
            paths.push(Path::new(arg));
        }
    }

    if paths.is_empty() {
        eprintln!("Usage: paste <file1> <file2> ...");
        std::process::exit(1);
    }

    let mut files = Vec::new();
    for path in paths {
        let file = File::open(path)?;
        files.push(BufReader::new(file));
    }

    let mut stdout = io::stdout().lock();
    let files_len = files.len();

    // Временный буфер для строк текущей итерации
    let mut lines = vec![Vec::new(); files_len];

    loop {
        let mut any_read = false;

        for (idx, reader) in files.iter_mut().enumerate() {
            lines[idx].clear();
            let n = reader.read_until(b'\n', &mut lines[idx])?;
            if n > 0 {
                any_read = true;
                // Удаляем завершающий символ переноса строки для форматирования
                let len = lines[idx].len();
                if len > 0 && lines[idx][len - 1] == b'\n' {
                    lines[idx].pop();
                }
            }
        }

        // Если все файлы достигли EOF — мгновенно и чисто выходим
        if !any_read {
            break;
        }

        // Выводим строки, строго разделяя их знаком табуляции
        for (idx, line) in lines.iter().enumerate() {
            stdout.write_all(line)?;
            if idx < files_len - 1 {
                stdout.write_all(b"\t")?;
            }
        }
        stdout.write_all(b"\n")?;
    }
    Ok(())
}
