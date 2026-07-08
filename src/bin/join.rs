use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 3 {
        eprintln!("Usage: join <file1> <file2>");
        std::process::exit(1);
    }

    let file1 = BufReader::new(File::open(Path::new(&args[1]))?);
    let file2 = BufReader::new(File::open(Path::new(&args[2]))?);

    let mut map = HashMap::new();

    for line in file1.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let Some(&key) = parts.first() {
            map.insert(key.to_string(), parts[1..].join(" "));
        }
    }

    for line in file2.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let Some(&key) = parts.first() {
            if let Some(val1) = map.get(key) {
                let val2 = parts[1..].join(" ");
                println!("{} {} {}", key, val1, val2);
            }
        }
    }
    Ok(())
}
