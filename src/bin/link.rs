use std::env;
use std::ffi::CString;
use std::io;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() != 3 {
        eprintln!("Usage: link <target> <link_name>");
        std::process::exit(1);
    }

    let target = args[1].to_string_lossy();
    let link_name = args[2].to_string_lossy();

    let c_target = CString::new(target.as_bytes()).unwrap();
    let c_link = CString::new(link_name.as_bytes()).unwrap();

    if unsafe { libc::link(c_target.as_ptr(), c_link.as_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
