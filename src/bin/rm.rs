use std::env;
use std::fs;
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut recursive = false;
    let mut force = false;
    let mut paths = Vec::new();

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') && s != "-" {
            for c in s.chars().skip(1) {
                match c {
                    'r' | 'R' => recursive = true,
                    'f' => force = true,
                    _ => {
                        eprintln!("rm: invalid option: -{}", c);
                        std::process::exit(1);
                    }
                }
            }
        } else {
            paths.push(arg);
        }
    }

    if paths.is_empty() {
        if force {
            return Ok(());
        }
        eprintln!("Usage: rm [-rf] <file1> ...");
        std::process::exit(1);
    }

    for path in paths {
        let path_ref = Path::new(&path);
        let metadata = fs::symlink_metadata(path_ref);
        match metadata {
            Ok(meta) => {
                if meta.is_dir() {
                    if recursive {
                        fs::remove_dir_all(path_ref)?;
                    } else {
                        eprintln!("rm: {}: Is a directory", path_ref.to_string_lossy());
                        std::process::exit(1);
                    }
                } else {
                    fs::remove_file(path_ref)?;
                }
            }
            Err(e) => {
                if !force {
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}
