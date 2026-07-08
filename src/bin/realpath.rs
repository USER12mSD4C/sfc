use std::env;
use std::fs;
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        eprintln!("Usage: realpath <path>");
        std::process::exit(1);
    }
    for arg in args.iter().skip(1) {
        let path = Path::new(arg);
        let canonical = fs::canonicalize(path)?;
        println!("{}", canonical.to_string_lossy());
    }
    Ok(())
}
