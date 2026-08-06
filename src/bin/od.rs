use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    let mut path_arg = None;
    let mut char_mode = false;

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') && s.len() > 1 {
            if s == "-c" {
                char_mode = true;
            } else if s == "--help" || s == "--version" {
                return ExitCode::from(0);
            }
        } else {
            path_arg = Some(arg.clone());
        }
    }

    let reader: Box<dyn Read> = if let Some(p) = path_arg {
        match File::open(Path::new(&p)) {
            Ok(f) => Box::new(f),
            Err(e) => {
                eprintln!("od: {}: {}", p.to_string_lossy(), e);
                return ExitCode::from(1);
            }
        }
    } else {
        Box::new(io::stdin())
    };

    let mut reader = reader;
    let mut buffer = [0u8; 16];
    let mut address = 0;

    loop {
        let n = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                eprintln!("od: read error: {}", e);
                return ExitCode::from(1);
            }
        };

        print!("{:07o}", address);

        if char_mode {
            for i in 0..n {
                let b = buffer[i];
                match b {
                    b'\\' => print!("  \\\\"),
                    b'\n' => print!("  \\n"),
                    b'\r' => print!("  \\r"),
                    b'\t' => print!("  \\t"),
                    0x00..=0x07 | 0x0e..=0x1f | 0x7f => print!(" {:03o}", b),
                    _ => print!("   {}", b as char),
                }
            }
            println!();
        } else {
            let mut i = 0;
            while i < n {
                let b0 = buffer[i];
                let b1 = if i + 1 < n { buffer[i + 1] } else { 0 };
                let word = u16::from_ne_bytes([b0, b1]);
                print!(" {:06o}", word);
                i += 2;
            }
            println!();
        }
        address += n;
    }

    if address > 0 {
        println!("{:07o}", address);
    }
    ExitCode::from(0)
}
