use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;

fn make_suffix(mut num: usize) -> String {
    let mut suffix = String::new();
    for _ in 0..2 {
        let c = ((num % 26) as u8 + b'a') as char;
        suffix.insert(0, c);
        num /= 26;
    }
    suffix
}

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut split_lines = Some(1000);
    let mut split_bytes = None;
    let mut file_path = None;

    let mut i = 1;
    while i < args.len() {
        let s = args[i].to_string_lossy();
        if s == "-l" && i + 1 < args.len() {
            split_lines = Some(args[i + 1].to_string_lossy().parse().unwrap_or(1000));
            split_bytes = None;
            i += 2;
        } else if s == "-b" && i + 1 < args.len() {
            let b_str = args[i + 1].to_string_lossy();
            let bytes_count = if b_str.ends_with('k') || b_str.ends_with('K') {
                b_str[..b_str.len() - 1].parse::<usize>().unwrap_or(1) * 1024
            } else if b_str.ends_with('m') || b_str.ends_with('M') {
                b_str[..b_str.len() - 1].parse::<usize>().unwrap_or(1) * 1024 * 1024
            } else {
                b_str.parse::<usize>().unwrap_or(1)
            };
            split_bytes = Some(bytes_count);
            split_lines = None;
            i += 2;
        } else if s.starts_with('-') {
            eprintln!("split: invalid option: {}", s);
            std::process::exit(1);
        } else {
            file_path = Some(s.into_owned());
            i += 1;
        }
    }

    let reader: Box<dyn Read> = if let Some(ref path) = file_path {
        Box::new(File::open(Path::new(path))?)
    } else {
        Box::new(io::stdin())
    };

    let mut suffix_num = 0;
    if let Some(bytes_limit) = split_bytes {
        let mut buffer = vec![0u8; 16384];
        let mut current_file_bytes = 0;
        let mut current_writer: Option<File> = None;

        let mut r = reader;
        loop {
            let n = r.read(&mut buffer)?;
            if n == 0 {
                break;
            }

            let mut offset = 0;
            while offset < n {
                if current_writer.is_none() {
                    let filename = format!("x{}", make_suffix(suffix_num));
                    current_writer = Some(File::create(filename)?);
                    suffix_num += 1;
                    current_file_bytes = 0;
                }

                let remaining_in_file = bytes_limit - current_file_bytes;
                let to_write = std::cmp::min(n - offset, remaining_in_file);

                current_writer
                    .as_mut()
                    .unwrap()
                    .write_all(&buffer[offset..offset + to_write])?;
                offset += to_write;
                current_file_bytes += to_write;

                if current_file_bytes >= bytes_limit {
                    current_writer = None;
                }
            }
        }
    } else if let Some(lines_limit) = split_lines {
        let mut buf_reader = BufReader::new(reader);
        let mut line = Vec::new();
        let mut current_file_lines = 0;
        let mut current_writer: Option<File> = None;

        loop {
            line.clear();
            let n = buf_reader.read_until(b'\n', &mut line)?;
            if n == 0 {
                break;
            }

            if current_writer.is_none() {
                let filename = format!("x{}", make_suffix(suffix_num));
                current_writer = Some(File::create(filename)?);
                suffix_num += 1;
                current_file_lines = 0;
            }

            current_writer.as_mut().unwrap().write_all(&line)?;
            current_file_lines += 1;

            if current_file_lines >= lines_limit {
                current_writer = None;
            }
        }
    }

    Ok(())
}
