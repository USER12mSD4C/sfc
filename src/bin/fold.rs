use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut width = 80;
    let mut file_path = None;

    let mut i = 1;
    while i < args.len() {
        let s = args[i].to_string_lossy();
        if s == "-w" && i + 1 < args.len() {
            width = args[i + 1].to_string_lossy().parse().unwrap_or(80);
            i += 2;
        } else if s.starts_with('-') {
            eprintln!("fold: invalid option: {}", s);
            std::process::exit(1);
        } else {
            file_path = Some(s.into_owned());
            i += 1;
        }
    }

    let reader: Box<dyn BufRead> = if let Some(ref path) = file_path {
        Box::new(BufReader::new(File::open(Path::new(path))?))
    } else {
        Box::new(BufReader::new(io::stdin()))
    };

    let mut stdout = io::stdout().lock();
    let mut line = Vec::new();
    let mut r = reader;

    loop {
        line.clear();
        let n = r.read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }

        let mut len = line.len();
        let mut has_newline = false;
        if len > 0 && line[len - 1] == b'\n' {
            len -= 1;
            has_newline = true;
        }

        let mut offset = 0;
        while offset < len {
            let chunk_len = std::cmp::min(width, len - offset);
            stdout.write_all(&line[offset..offset + chunk_len])?;
            offset += chunk_len;
            if offset < len {
                stdout.write_all(b"\n")?;
            }
        }
        if has_newline {
            stdout.write_all(b"\n")?;
        }
    }
    Ok(())
}
