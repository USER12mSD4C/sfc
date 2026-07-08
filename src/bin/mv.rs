use std::env;
use std::fs;
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut no_clobber = false;
    let mut no_target_dir = false;
    let mut targets = Vec::new();

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') && s != "-" {
            for c in s.chars().skip(1) {
                match c {
                    'f' => no_clobber = false,
                    'n' => no_clobber = true,
                    'T' => no_target_dir = true,
                    _ => {
                        eprintln!("mv: invalid option: -{}", c);
                        std::process::exit(1);
                    }
                }
            }
        } else {
            targets.push(Path::new(arg));
        }
    }

    if targets.len() < 2 {
        eprintln!("Usage: mv [-fnT] <source> <target>");
        std::process::exit(1);
    }

    let source = targets[0];
    let mut dest = targets[1].to_path_buf();

    if dest.is_dir() && !no_target_dir {
        if let Some(file_name) = source.file_name() {
            dest.push(file_name);
        }
    }

    if no_clobber && dest.exists() {
        return Ok(());
    }

    match fs::rename(source, &dest) {
        Ok(_) => {}
        Err(ref e) if e.raw_os_error() == Some(libc::EXDEV) => {
            copy_and_remove(source, &dest)?;
        }
        Err(e) => return Err(e),
    }

    Ok(())
}

fn copy_and_remove(src: &Path, dest: &Path) -> io::Result<()> {
    let meta = fs::metadata(src)?;
    if meta.is_dir() {
        copy_dir_recursive(src, dest)?;
        fs::remove_dir_all(src)?;
    } else {
        fs::copy(src, dest)?;
        fs::remove_file(src)?;
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}
