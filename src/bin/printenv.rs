use std::env;
use std::io::{self, Write};

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdout = io::stdout().lock();

    if args.is_empty() {
        for (key, val) in env::vars_os() {
            stdout.write_all(key.as_encoded_bytes())?;
            stdout.write_all(b"=")?;
            stdout.write_all(val.as_encoded_bytes())?;
            stdout.write_all(b"\n")?;
        }
    } else {
        for arg in args {
            if let Some(val) = env::var_os(&arg) {
                stdout.write_all(val.as_encoded_bytes())?;
                stdout.write_all(b"\n")?;
            } else {
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
