use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut t = 0;
    unsafe {
        libc::time(&mut t);
    }
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    let mut buf = [0u8; 128];

    // Формат GNU Date: "Wed Jul  8 17:41:00 CEST 2026"
    let fmt = b"%a %b %e %H:%M:%S %Z %Y\0";
    let len = unsafe {
        libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            fmt.as_ptr() as *const libc::c_char,
            &tm,
        )
    };
    if len > 0 {
        let mut stdout = io::stdout().lock();
        stdout.write_all(&buf[..len])?;
        stdout.write_all(b"\n")?;
    }
    Ok(())
}
