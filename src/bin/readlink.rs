use std::env;
use std::fs;
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut canonicalize = false;
    let mut paths = Vec::new();

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "-f" {
            canonicalize = true;
        } else if s.starts_with('-') {
            eprintln!("readlink: invalid option: {}", s);
            std::process::exit(1);
        } else {
            paths.push(Path::new(arg));
        }
    }

    if paths.is_empty() {
        eprintln!("Usage: readlink [-f] <path>");
        std::process::exit(1);
    }

    for path in paths {
        if canonicalize {
            let res = fs::canonicalize(path)?;
            println!("{}", res.to_string_lossy());
        } else {
            let res = fs::read_link(path)?;
            println!("{}", res.to_string_lossy());
        }
    }
    Ok(())
}
