use std::io;

fn main() -> io::Result<()> {
    unsafe {
        let mut termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut termios) < 0 {
            return Err(io::Error::last_os_error());
        }

        let speed = libc::cfgetospeed(&termios);
        let baud = match speed {
            libc::B9600 => "9600",
            libc::B38400 => "38400",
            libc::B115200 => "115200",
            _ => "custom",
        };

        println!("speed {} baud;", baud);

        let lflag = termios.c_lflag;
        print!("lflags: ");
        if lflag & libc::ECHO != 0 {
            print!("echo ");
        } else {
            print!("-echo ");
        }
        if lflag & libc::ICANON != 0 {
            print!("icanon ");
        } else {
            print!("-icanon ");
        }
        if lflag & libc::ISIG != 0 {
            print!("isig ");
        } else {
            print!("-isig ");
        }
        println!();
    }
    Ok(())
}
