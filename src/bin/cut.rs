use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut delimiter = b'\t';
    let mut field = 0; // 0-индексация (колонка 1)
    let mut paths = Vec::new();

    let mut i = 1;
    while i < args.len() {
        let s = args[i].to_string_lossy();
        if s == "-d" && i + 1 < args.len() {
            let d_str = args[i + 1].to_string_lossy();
            if let Some(first_char) = d_str.bytes().next() {
                delimiter = first_char;
            }
            i += 2;
        } else if s == "-f" && i + 1 < args.len() {
            field = args[i + 1]
                .to_string_lossy()
                .parse::<usize>()
                .unwrap_or(1)
                .saturating_sub(1);
            i += 2;
        } else {
            paths.push(Path::new(&args[i]));
            i += 1;
        }
    }

    let mut stdout = io::stdout().lock();

    if paths.is_empty() {
        let stdin = io::stdin();
        process_cut(stdin.lock(), &mut stdout, delimiter, field)?;
    } else {
        for path in paths {
            let file = File::open(path)?;
            process_cut(BufReader::new(file), &mut stdout, delimiter, field)?;
        }
    }
    Ok(())
}

fn process_cut<R: BufRead, W: Write>(
    mut reader: R,
    writer: &mut W,
    delim: u8,
    field: usize,
) -> io::Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }

        let mut len = line.len();
        if len > 0 && line[len - 1] == b'\n' {
            len -= 1;
        }
        if len > 0 && line[len - 1] == b'\r' {
            len -= 1;
        }

        let cols: Vec<&[u8]> = line[..len].split(|&b| b == delim).collect();
        if let Some(col) = cols.get(field) {
            writer.write_all(col)?;
        }
        writer.write_all(b"\n")?;
    }
    Ok(())
}
