use std::env;
use std::fs::{self, File, Metadata};
use std::io::{self, Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: cp [-r] <source> <destination>");
        std::process::exit(1);
    }

    let (recursive, src_idx) = if args[1] == "-r" || args[1] == "--recursive" {
        if args.len() < 4 {
            eprintln!("Usage: cp [-r] <source> <destination>");
            std::process::exit(1);
        }
        (true, 2)
    } else {
        (false, 1)
    };

    let src = Path::new(&args[src_idx]);
    let dest = Path::new(&args[src_idx + 1]);

    let src_meta = fs::metadata(src)?;

    if src_meta.is_dir() {
        if !recursive {
            eprintln!("cp: omitting directory '{}'", src.display());
            eprintln!("cp: use -r to copy directories");
            std::process::exit(1);
        }
        copy_dir(src, dest)?;
    } else {
        copy_file(src, dest, &src_meta)?;
    }

    Ok(())
}

fn copy_file(src: &Path, dest: &Path, _meta: &Metadata) -> io::Result<()> {
    // If dest is an existing directory, place inside it: dest/src_filename
    let dest = if dest.is_dir() {
        dest.join(src.file_name().unwrap_or(src.as_os_str()))
    } else {
        dest.to_path_buf()
    };

    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let src_file = File::open(src)?;
    let dest_file = File::create(&dest)?;

    let src_fd = src_file.as_raw_fd();
    let dest_fd = dest_file.as_raw_fd();

    let mut total_copied = 0usize;
    loop {
        let res = unsafe {
            libc::copy_file_range(
                src_fd,
                std::ptr::null_mut(),
                dest_fd,
                std::ptr::null_mut(),
                1024 * 1024,
                0,
            )
        };

        if res < 0 {
            let err = io::Error::last_os_error();
            let raw_err = err.raw_os_error();
            if total_copied == 0
                && (raw_err == Some(libc::ENOSYS)
                    || raw_err == Some(libc::EXDEV)
                    || raw_err == Some(libc::EINVAL))
            {
                return fallback_copy(src_file, dest_file);
            }
            return Err(err);
        } else if res == 0 {
            break;
        }
        total_copied += res as usize;
    }

    Ok(())
}

fn fallback_copy(mut src: File, mut dest: File) -> io::Result<()> {
    let mut buffer = [0u8; 16384];
    loop {
        let n = src.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        dest.write_all(&buffer[..n])?;
    }
    Ok(())
}

fn copy_dir(src: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        let meta = entry.metadata()?;

        if meta.is_dir() {
            copy_dir(&src_path, &dest_path)?;
        } else {
            copy_file(&src_path, &dest_path, &meta)?;
        }
    }
    Ok(())
}
