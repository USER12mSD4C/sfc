use std::env;
use std::fs;
use std::io;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut parents = false;
    let mut paths = Vec::new();

    for arg in args.iter().skip(1) {
        if arg == "-p" {
            parents = true;
        } else {
            paths.push(arg);
        }
    }

    if paths.is_empty() {
        eprintln!("Использование: sfmkdir [-p] <директория1> ...");
        std::process::exit(1);
    }

    for path in paths {
        if parents {
            fs::create_dir_all(path)?;
        } else {
            fs::create_dir(path)?;
        }
    }
    Ok(())
}
