use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut tab_size = 8;
    let mut file_path = None;

    let mut i = 1;
    while i < args.len() {
        let s = args[i].to_string_lossy();
        if s == "-t" && i + 1 < args.len() {
            tab_size = args[i + 1].to_string_lossy().parse().unwrap_or(8);
            i += 2;
        } else if s.starts_with('-') {
            eprintln!("expand: invalid option: {}", s);
            std::process::exit(1);
        } else {
            file_path = Some(s.into_owned());
            i += 1;
        }
    }

    let mut reader: Box<dyn Read> = if let Some(ref path) = file_path {
        Box::new(File::open(Path::new(path))?)
    } else {
        Box::new(io::stdin())
    };

    let mut stdout = io::stdout().lock();
    let mut buffer = [0u8; 16384];
    let mut col = 0;

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }

        for &b in &buffer[..n] {
            if b == b'\t' {
                let spaces = tab_size - (col % tab_size);
                for _ in 0..spaces {
                    stdout.write_all(b" ")?;
                }
                col += spaces;
            } else {
                stdout.write_all(&[b])?;
                if b == b'\n' {
                    col = 0;
                } else {
                    col += 1;
                }
            }
        }
    }
    Ok(())
}
