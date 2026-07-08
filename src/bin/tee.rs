use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut files = Vec::new();
    for arg in args.iter().skip(1) {
        let file = File::create(Path::new(arg))?;
        files.push(file);
    }

    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut stdout = io::stdout().lock();
    let mut buffer = [0u8; 16384];

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }

        stdout.write_all(&buffer[..n])?;
        for file in &mut files {
            file.write_all(&buffer[..n])?;
        }
    }
    Ok(())
}
