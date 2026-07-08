use std::env;
use std::ffi::CString;
use std::os::unix::fs::chown;
use std::path::Path;

fn get_uid(name: &str) -> Option<u32> {
    if let Ok(uid) = name.parse::<u32>() {
        return Some(uid);
    }
    let c_name = CString::new(name).ok()?;
    unsafe {
        let pwd = libc::getpwnam(c_name.as_ptr());
        if !pwd.is_null() {
            Some((*pwd).pw_uid)
        } else {
            None
        }
    }
}

fn get_gid(name: &str) -> Option<u32> {
    if let Ok(gid) = name.parse::<u32>() {
        return Some(gid);
    }
    let c_name = CString::new(name).ok()?;
    unsafe {
        let grp = libc::getgrnam(c_name.as_ptr());
        if !grp.is_null() {
            Some((*grp).gr_gid)
        } else {
            None
        }
    }
}

fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 3 {
        eprintln!("Usage: chown [owner][:group] <file1> ...");
        std::process::exit(1);
    }

    let spec = args[1].to_string_lossy();
    let files = &args[2..];

    let mut uid = None;
    let mut gid = None;

    if spec.contains(':') {
        let parts: Vec<&str> = spec.splitn(2, ':').collect();
        if !parts[0].is_empty() {
            uid = get_uid(parts[0]);
        }
        if !parts[1].is_empty() {
            gid = get_gid(parts[1]);
        }
    } else {
        uid = get_uid(&spec);
    }

    for file in files {
        let path = Path::new(file);
        if let Err(e) = chown(path, uid, gid) {
            eprintln!("chown: {}: {}", path.to_string_lossy(), e);
        }
    }
}
