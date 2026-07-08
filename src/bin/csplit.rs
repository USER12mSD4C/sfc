use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 3 {
        eprintln!("Usage: csplit <file> <line1> [line2 ...]");
        std::process::exit(1);
    }

    let file_path = Path::new(&args[1]);
    let reader = BufReader::new(File::open(file_path)?);

    let mut split_lines = Vec::new();
    for arg in args.iter().skip(2) {
        if let Ok(line) = arg.to_string_lossy().parse::<usize>() {
            split_lines.push(line);
        } else {
            eprintln!("csplit: invalid line number: {}", arg.to_string_lossy());
            std::process::exit(1);
        }
    }
    split_lines.sort_unstable();

    let mut split_iter = split_lines.into_iter().peekable();
    let mut file_idx = 0;
    let mut line_num = 1;
    let mut current_writer = File::create(format!("xx{:02}", file_idx))?;
    file_idx += 1;

    for line in reader.lines() {
        let line = line?;
        if let Some(&next_split) = split_iter.peek() {
            if line_num == next_split {
                split_iter.next();
                current_writer = File::create(format!("xx{:02}", file_idx))?;
                file_idx += 1;
            }
        }
        writeln!(current_writer, "{}", line)?;
        line_num += 1;
    }

    Ok(())
}
