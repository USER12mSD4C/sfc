use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut paths = Vec::new();
    for arg in args.iter().skip(1) {
        paths.push(Path::new(arg));
    }

    let mut stdout = io::stdout();

    if paths.is_empty() {
        let stdin = io::stdin();
        process_format(stdin.lock(), &mut stdout)?;
    } else {
        for path in paths {
            let file = File::open(path)?;
            process_format(BufReader::new(file), &mut stdout)?;
        }
    }
    Ok(())
}

fn process_format<R: BufRead, W: io::Write>(mut reader: R, writer: &mut W) -> io::Result<()> {
    let mut col = 0;
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }

        for word in line.split_whitespace() {
            if col + word.len() + 1 > 75 {
                writeln!(writer)?;
                write!(writer, "{}", word)?;
                col = word.len();
            } else {
                if col > 0 {
                    write!(writer, " ")?;
                    col += 1;
                }
                write!(writer, "{}", word)?;
                col += word.len();
            }
        }
    }
    if col > 0 {
        writeln!(writer)?;
    }
    Ok(())
}
