use std::collections::VecDeque;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut lines_count = 10;
    let mut paths = Vec::new();

    let mut i = 1;
    while i < args.len() {
        let s = args[i].to_string_lossy();
        if s == "-n" && i + 1 < args.len() {
            lines_count = args[i + 1].to_string_lossy().parse().unwrap_or(10);
            i += 2;
        } else {
            paths.push(Path::new(&args[i]));
            i += 1;
        }
    }

    let mut stdout = io::stdout().lock();

    if paths.is_empty() {
        let stdin = io::stdin();
        process_reader(stdin.lock(), &mut stdout, lines_count)?;
    } else {
        for path in paths {
            let file = File::open(path)?;
            process_reader(BufReader::new(file), &mut stdout, lines_count)?;
        }
    }
    Ok(())
}

fn process_reader<R: BufRead, W: Write>(
    mut reader: R,
    writer: &mut W,
    max_lines: usize,
) -> io::Result<()> {
    let mut buffer = VecDeque::with_capacity(max_lines);
    loop {
        let mut line = Vec::new();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }
        if buffer.len() == max_lines {
            buffer.pop_front();
        }
        buffer.push_back(line);
    }
    for line in buffer {
        writer.write_all(&line)?;
    }
    Ok(())
}
