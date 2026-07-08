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

    let mut lines = Vec::new();

    if paths.is_empty() {
        let stdin = io::stdin();
        read_lines(stdin.lock(), &mut lines)?;
    } else {
        for path in paths {
            let file = File::open(path)?;
            read_lines(BufReader::new(file), &mut lines)?;
        }
    }

    let mut stdout = io::stdout().lock();
    for line in lines.iter().rev() {
        stdout.write_all(line)?;
    }
    Ok(())
}

fn read_lines<R: BufRead>(mut reader: R, lines: &mut Vec<Vec<u8>>) -> io::Result<()> {
    loop {
        let mut line = Vec::new();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }
        lines.push(line);
    }
    Ok(())
}
