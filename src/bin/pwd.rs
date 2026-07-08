use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut buf = [0u8; 4096];

    let ptr = unsafe { libc::getcwd(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };

    if ptr.is_null() {
        return Err(io::Error::last_os_error());
    }

    let len = unsafe { libc::strlen(ptr) };

    let mut stdout = io::stdout().lock();
    stdout.write_all(&buf[..len])?;
    stdout.write_all(b"\n")?;

    Ok(())
}
