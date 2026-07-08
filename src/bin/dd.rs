use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut input_path = None;
    let mut output_path = None;
    let mut block_size = 512;
    let mut count = None;

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with("if=") {
            input_path = Some(s["if=".len()..].to_string());
        } else if s.starts_with("of=") {
            output_path = Some(s["of=".len()..].to_string());
        } else if s.starts_with("bs=") {
            block_size = s["bs=".len()..].parse::<usize>().unwrap_or(512);
        } else if s.starts_with("count=") {
            count = Some(s["count=".len()..].parse::<usize>().unwrap_or(0));
        }
    }

    let mut reader: Box<dyn Read> = if let Some(ref path) = input_path {
        Box::new(File::open(Path::new(path))?)
    } else {
        Box::new(io::stdin())
    };

    let mut writer: Box<dyn Write> = if let Some(ref path) = output_path {
        Box::new(File::create(Path::new(path))?)
    } else {
        Box::new(io::stdout())
    };

    let mut buffer = vec![0u8; block_size];
    let mut blocks_copied = 0;

    loop {
        if let Some(c) = count {
            if blocks_copied >= c {
                break;
            }
        }

        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buffer[..n])?;
        blocks_copied += 1;
    }

    eprintln!("{}+0 records in", blocks_copied);
    eprintln!("{}+0 records out", blocks_copied);
    Ok(())
}
