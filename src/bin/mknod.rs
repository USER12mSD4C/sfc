use std::env;
use std::ffi::CString;
use std::io;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 3 {
        eprintln!("Usage: mknod <name> <type> [major minor]");
        eprintln!("Type can be: b (block), c (character), p (fifo)");
        std::process::exit(1);
    }

    let name = args[1].to_string_lossy();
    let type_str = args[2].to_string_lossy();

    let mut mode = 0o666;
    let mut dev = 0;

    match type_str.as_ref() {
        "p" => {
            mode |= libc::S_IFIFO as u32;
        }
        "b" | "c" => {
            if args.len() < 5 {
                eprintln!("mknod: major and minor device numbers required for device files");
                std::process::exit(1);
            }
            let major = args[3].to_string_lossy().parse::<u64>().unwrap_or(0);
            let minor = args[4].to_string_lossy().parse::<u64>().unwrap_or(0);

            // ИСПРАВЛЕНИЕ: убран ненужный небезопасный блок unsafe вокруг makedev
            dev = libc::makedev(major as libc::c_uint, minor as libc::c_uint);
            if type_str == "b" {
                mode |= libc::S_IFBLK as u32;
            } else {
                mode |= libc::S_IFCHR as u32;
            }
        }
        _ => {
            eprintln!("mknod: invalid type: {}", type_str);
            std::process::exit(1);
        }
    }

    let c_name = CString::new(name.as_bytes()).unwrap();
    if unsafe { libc::mknod(c_name.as_ptr(), mode as libc::mode_t, dev as libc::dev_t) } < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}
