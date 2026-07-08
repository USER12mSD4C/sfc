use std::io::{self, Write};

fn main() -> io::Result<()> {
    let is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) } != 0;
    if is_tty {
        let mut buf = [0u8; 1024];
        let res = unsafe {
            libc::ttyname_r(
                libc::STDIN_FILENO,
                buf.as_mut_ptr() as *mut libc::c_char,
                buf.len(),
            )
        };
        if res == 0 {
            let len = unsafe { libc::strlen(buf.as_ptr() as *const libc::c_char) };
            let mut stdout = io::stdout().lock();
            stdout.write_all(&buf[..len])?;
            stdout.write_all(b"\n")?;
            std::process::exit(0);
        }
    }
    println!("not a tty");
    std::process::exit(1);
}
