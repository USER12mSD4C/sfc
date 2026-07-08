use std::env;
use std::ffi::CString;
use std::io;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        eprintln!("Usage: mkfifo <path>");
        std::process::exit(1);
    }

    let path = args[1].to_string_lossy();
    let c_path = CString::new(path.as_bytes()).unwrap();

    // Права доступа по умолчанию: 0666
    if unsafe { libc::mkfifo(c_path.as_ptr(), 0o666) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
