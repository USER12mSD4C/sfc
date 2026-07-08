use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};

fn main() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let args: Vec<_> = env::args_os().collect();

    let mut number_lines = false;
    let mut paths = Vec::new();

    for arg in args.iter().skip(1) {
        if arg == "-n" {
            number_lines = true;
        } else {
            paths.push(arg);
        }
    }

    let mut line_counter = 1;

    if paths.is_empty() {
        let stdin = io::stdin();
        process_reader(stdin.lock(), &mut stdout, number_lines, &mut line_counter)?;
    } else {
        for path in paths {
            let file = match File::open(path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("sfcat: {}: {}", path.to_string_lossy(), e);
                    continue;
                }
            };

            if number_lines {
                let reader = BufReader::new(file);
                process_reader(reader, &mut stdout, number_lines, &mut line_counter)?;
            } else {
                let mut raw_file = file;
                let mut buffer = [0u8; 16384];
                loop {
                    let n = raw_file.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    stdout.write_all(&buffer[..n])?;
                }
            }
        }
    }
    Ok(())
}

fn process_reader<R: BufRead, W: Write>(
    mut reader: R,
    writer: &mut W,
    number_lines: bool,
    line_counter: &mut usize,
) -> io::Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }

        if number_lines {
            write!(writer, "{:6}\t", line_counter)?;
            *line_counter += 1;
        }
        writer.write_all(&line)?;
    }
    Ok(())
}
