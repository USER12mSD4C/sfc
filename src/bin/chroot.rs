use std::env;
use std::ffi::CString;
use std::process::Command;

fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        eprintln!("Usage: chroot <new_root> [command [argument ...]]");
        std::process::exit(1); // Ошибка неверного синтаксиса аргументов
    }

    let new_root = &args[1];
    let c_root = CString::new(new_root.to_string_lossy().as_bytes()).unwrap();

    if unsafe { libc::chroot(c_root.as_ptr()) } < 0 {
        eprintln!(
            "chroot: cannot change root directory: {}",
            std::io::Error::last_os_error()
        );
        std::process::exit(125); // Согласно стандартам GNU при ошибке привилегий/системы возвращаем 125
    }

    if let Err(e) = env::set_current_dir("/") {
        eprintln!("chroot: cannot change directory to /: {}", e);
        std::process::exit(125);
    }

    let mut cmd = if args.len() > 2 {
        Command::new(&args[2])
    } else {
        Command::new("/bin/sh")
    };

    if args.len() > 3 {
        cmd.args(&args[3..]);
    }

    match cmd.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(0)),
        Err(e) => {
            eprintln!("chroot: failed to run command: {}", e);
            std::process::exit(127); // Команда не найдена
        }
    }
}
