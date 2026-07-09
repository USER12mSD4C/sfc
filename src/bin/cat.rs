use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let mut number_lines = false;
    let mut files = Vec::new();

    for arg in args.iter().skip(1) {
        if arg == "-n" || arg == "--number" {
            number_lines = true;
        } else {
            files.push(arg);
        }
    }

    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();
    let mut line_counter = 1;

    if files.is_empty() {
        let stdin = io::stdin();
        let mut stdin_lock = stdin.lock();
        process(
            &mut stdin_lock,
            &mut stdout_lock,
            number_lines,
            &mut line_counter,
        )?;
    } else {
        for file_path in files {
            if file_path == "-" {
                let stdin = io::stdin();
                let mut stdin_lock = stdin.lock();
                process(
                    &mut stdin_lock,
                    &mut stdout_lock,
                    number_lines,
                    &mut line_counter,
                )?;
            } else {
                match File::open(file_path) {
                    Ok(file) => {
                        let mut reader = BufReader::new(file);
                        process(
                            &mut reader,
                            &mut stdout_lock,
                            number_lines,
                            &mut line_counter,
                        )?;
                    }
                    Err(e) => {
                        eprintln!("cat: {}: {}", file_path, e);
                    }
                }
            }
        }
    }
    stdout_lock.flush()?;
    Ok(())
}

fn process<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    number_lines: bool,
    line_counter: &mut usize,
) -> io::Result<()> {
    if number_lines {
        let mut buf_reader = BufReader::new(reader);
        let mut line = Vec::new();
        loop {
            line.clear();
            let n = buf_reader.read_until(b'\n', &mut line)?;
            if n == 0 {
                break;
            }
            write!(writer, "{:6}\t", line_counter)?;
            *line_counter += 1;
            writer.write_all(&line)?;
            // Принудительно выталкиваем данные на экран
            writer.flush()?;
        }
    } else {
        let mut buffer = [0u8; 16384];
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            writer.write_all(&buffer[..n])?;
            // Принудительно выталкиваем данные для интерактивных пайпов
            writer.flush()?;
        }
    }
    Ok(())
}
