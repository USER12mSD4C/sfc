use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::io::AsRawFd;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 3 {
        eprintln!("Использование: sfcp <откуда> <куда>");
        std::process::exit(1);
    }

    let src_path = &args[1];
    let dest_path = &args[2];

    let src_file = File::open(src_path)?;
    let dest_file = File::create(dest_path)?;

    let src_fd = src_file.as_raw_fd();
    let dest_fd = dest_file.as_raw_fd();

    let mut total_copied = 0;
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
        total_copied += res;
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
