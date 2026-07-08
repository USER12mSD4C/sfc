use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut paths = Vec::new();
    for arg in args.iter().skip(1) {
        paths.push(Path::new(arg));
    }

    let mut stdout = io::stdout().lock();

    if paths.is_empty() {
        let stdin = io::stdin();
        process_uniq(stdin.lock(), &mut stdout)?;
    } else {
        for path in paths {
            let file = File::open(path)?;
            process_uniq(BufReader::new(file), &mut stdout)?;
        }
    }
    Ok(())
}

fn process_uniq<R: BufRead, W: Write>(mut reader: R, writer: &mut W) -> io::Result<()> {
    let mut prev_line = Vec::new();
    let mut current_line = Vec::new();
    let mut is_first = true;

    loop {
        current_line.clear();
        let n = reader.read_until(b'\n', &mut current_line)?;
        if n == 0 {
            break;
        }

        if is_first || current_line != prev_line {
            writer.write_all(&current_line)?;
            prev_line = current_line.clone();
            is_first = false;
        }
    }
    Ok(())
}
