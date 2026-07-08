use std::env;
use std::ffi::CString;
use std::io;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() != 2 {
        eprintln!("Usage: unlink <file>");
        std::process::exit(1);
    }

    let path_str = args[1].to_string_lossy();
    let c_path = CString::new(path_str.as_bytes()).unwrap();

    if unsafe { libc::unlink(c_path.as_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
