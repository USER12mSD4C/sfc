use std::env;
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;

fn expand_set(bytes: &[u8]) -> Vec<u8> {
    let mut expanded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 2 < bytes.len() && bytes[i+1] == b'-' {
            let start = bytes[i];
            let end = bytes[i+2];
            if start <= end {
                for b in start..=end {
                    expanded.push(b);
                }
            } else {
                for b in (end..=start).rev() {
                    expanded.push(b);
                }
            }
            i += 3;
        } else {
            expanded.push(bytes[i]);
            i += 1;
        }
    }
    expanded
}

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 3 {
        eprintln!("Usage: tr <set1> <set2>");
        std::process::exit(1);
    }

    // Извлекаем сырые байты аргументов Unix без UTF-8 валидации
    let set1 = expand_set(args[1].as_bytes());
    let set2 = expand_set(args[2].as_bytes());

    let mut map = [0u8; 256];
    for i in 0..256 {
        map[i] = i as u8;
    }

    for (i, &b1) in set1.iter().enumerate() {
        if let Some(&b2) = set2.get(i) {
            map[b1 as usize] = b2;
        } else if let Some(&last) = set2.last() {
            map[b1 as usize] = last;
        }
    }

    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut stdout = io::stdout().lock();
    let mut buffer = [0u8; 16384];

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 { break; }
        for b in &mut buffer[..n] {
            *b = map[*b as usize];
        }
        stdout.write_all(&buffer[..n])?;
    }
    Ok(())
}
