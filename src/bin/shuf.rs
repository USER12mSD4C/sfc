use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

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

    if lines.is_empty() {
        return Ok(());
    }

    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    if seed == 0 {
        seed = 1;
    }

    let n = lines.len();
    for i in (1..n).rev() {
        let rand_val = xorshift64(&mut seed);
        let j = (rand_val % (i as u64 + 1)) as usize;
        lines.swap(i, j);
    }

    let mut stdout = io::stdout().lock();
    for line in lines {
        stdout.write_all(&line)?;
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
