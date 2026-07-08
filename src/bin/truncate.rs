use std::env;
use std::fs::OpenOptions;
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut size_str = None;
    let mut file_path = None;

    let mut i = 1;
    while i < args.len() {
        let s = args[i].to_string_lossy();
        if s == "-s" && i + 1 < args.len() {
            size_str = Some(args[i + 1].to_string_lossy().into_owned());
            i += 2;
        } else if s.starts_with('-') {
            eprintln!("truncate: invalid option: {}", s);
            std::process::exit(1);
        } else {
            file_path = Some(s.into_owned());
            i += 1;
        }
    }

    let size_str = match size_str {
        Some(s) => s,
        None => {
            eprintln!("truncate: option required: -s");
            std::process::exit(1);
        }
    };

    let file_path = match file_path {
        Some(p) => p,
        None => {
            eprintln!("truncate: missing file operand");
            std::process::exit(1);
        }
    };

    let mut multiplier = 1u64;
    let mut num_part = size_str.as_str();
    if size_str.ends_with('K') || size_str.ends_with('k') {
        multiplier = 1024;
        num_part = &size_str[..size_str.len() - 1];
    } else if size_str.ends_with('M') || size_str.ends_with('m') {
        multiplier = 1024 * 1024;
        num_part = &size_str[..size_str.len() - 1];
    } else if size_str.ends_with('G') || size_str.ends_with('g') {
        multiplier = 1024 * 1024 * 1024;
        num_part = &size_str[..size_str.len() - 1];
    }

    let size = match num_part.parse::<u64>() {
        Ok(val) => val * multiplier,
        Err(_) => {
            eprintln!("truncate: invalid size: {}", size_str);
            std::process::exit(1);
        }
    };
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(Path::new(&file_path))?;
    file.set_len(size)?;

    Ok(())
}
