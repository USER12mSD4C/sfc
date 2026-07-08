use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 3 {
        eprintln!("Usage: comm <file1> <file2>");
        std::process::exit(1);
    }

    let mut f1 = BufReader::new(File::open(Path::new(&args[1]))?)
        .lines()
        .peekable();
    let mut f2 = BufReader::new(File::open(Path::new(&args[2]))?)
        .lines()
        .peekable();

    loop {
        match (f1.peek(), f2.peek()) {
            (Some(Ok(l1)), Some(Ok(l2))) => {
                if l1 < l2 {
                    println!("{}", l1);
                    let _ = f1.next();
                } else if l1 > l2 {
                    println!("\t{}", l2);
                    let _ = f2.next();
                } else {
                    println!("\t\t{}", l1);
                    let _ = f1.next();
                    let _ = f2.next();
                }
            }
            (Some(Ok(l1)), None) => {
                println!("{}", l1);
                let _ = f1.next();
            }
            (None, Some(Ok(l2))) => {
                println!("\t{}", l2);
                let _ = f2.next();
            }
            _ => break,
        }
    }
    Ok(())
}
