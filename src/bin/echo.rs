use std::env;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;

fn main() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let mut args = env::args_os().skip(1).peekable();

    let mut print_newline = true;
    if let Some(first) = args.peek() {
        if first.as_bytes() == b"-n" {
            print_newline = false;
            args.next();
        }
    }

    while let Some(arg) = args.next() {
        stdout.write_all(arg.as_bytes())?;
        if args.peek().is_some() {
            stdout.write_all(b" ")?;
        }
    }

    if print_newline {
        stdout.write_all(b"\n")?;
    }

    Ok(())
}
