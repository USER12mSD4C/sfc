use std::env;
use std::fs;
use std::io;
use std::os::unix::fs as unix_fs;
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut symbolic = false;
    let mut force = false;
    let mut no_deref = false;
    let mut targets = Vec::new();

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') && s != "-" {
            for c in s.chars().skip(1) {
                match c {
                    's' => symbolic = true,
                    'f' => force = true,
                    'n' => no_deref = true,
                    _ => {
                        eprintln!("ln: invalid option: -{}", c);
                        std::process::exit(1);
                    }
                }
            }
        } else {
            targets.push(Path::new(arg));
        }
    }

    if targets.len() < 2 {
        eprintln!("Usage: ln [-sfn] <source> <target>");
        std::process::exit(1);
    }

    let source = targets[0];
    let mut dest = targets[1].to_path_buf();

    if dest.is_dir() && !no_deref {
        if let Some(file_name) = source.file_name() {
            dest.push(file_name);
        }
    }

    if force {
        if fs::symlink_metadata(&dest).is_ok() {
            if dest.is_dir() && !dest.is_symlink() {
                fs::remove_dir_all(&dest)?;
            } else {
                fs::remove_file(&dest)?;
            }
        }
    }

    if symbolic {
        unix_fs::symlink(source, &dest)?;
    } else {
        fs::hard_link(source, &dest)?;
    }

    Ok(())
}
