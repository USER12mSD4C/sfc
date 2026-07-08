use std::env;
use std::fs::{FileTimes, OpenOptions};
use std::io;
use std::time::SystemTime;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        eprintln!("Usage: touch/mk <file1> [file2 ...]");
        std::process::exit(1);
    }

    for path in args.iter().skip(1) {
        let file = OpenOptions::new().write(true).create(true).open(path)?;

        let now = SystemTime::now();
        file.set_times(FileTimes::new().set_modified(now).set_accessed(now))?;
    }
    Ok(())
}
