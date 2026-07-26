use std::env;
use std::ffi::{CString, OsStr};
use std::os::unix::fs::chown;
use std::path::Path;
use std::process;

fn get_uid(name: &OsStr) -> Option<u32> {
    if let Some(s) = name.to_str() {
        if let Ok(uid) = s.parse::<u32>() {
            return Some(uid);
        }
    }
    let c_name = CString::new(name.as_encoded_bytes()).ok()?;
    unsafe {
        let pwd = libc::getpwnam(c_name.as_ptr());
        if !pwd.is_null() {
            Some((*pwd).pw_uid)
        } else {
            None
        }
    }
}

fn get_gid(name: &OsStr) -> Option<u32> {
    if let Some(s) = name.to_str() {
        if let Ok(gid) = s.parse::<u32>() {
            return Some(gid);
        }
    }
    let c_name = CString::new(name.as_encoded_bytes()).ok()?;
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
        process::exit(1);
    }

    let spec = &args[1];
    let files = &args[2..];

    let mut uid = None;
    let mut gid = None;
    let mut fatal = false;

    if let Some(spec_str) = spec.to_str() {
        if let Some(pos) = spec_str.find(':') {
            let (owner, group) = spec_str.split_at(pos);
            let group = &group[1..];

            if !owner.is_empty() {
                uid = get_uid(OsStr::new(owner));
                if uid.is_none() {
                    eprintln!("chown: invalid user: '{}'", owner);
                    fatal = true;
                }
            }
            if !group.is_empty() {
                gid = get_gid(OsStr::new(group));
                if gid.is_none() {
                    eprintln!("chown: invalid group: '{}'", group);
                    fatal = true;
                }
            }
        } else {
            if spec_str.is_empty() {
                eprintln!("chown: missing owner operand");
                fatal = true;
            } else {
                uid = get_uid(spec);
                if uid.is_none() {
                    eprintln!("chown: invalid user: '{}'", spec_str);
                    fatal = true;
                }
            }
        }
    } else {
        let lossy = spec.to_string_lossy();
        if let Some(pos) = lossy.find(':') {
            let (owner, group) = lossy.split_at(pos);
            let group = &group[1..];

            if !owner.is_empty() {
                uid = get_uid(OsStr::new(owner));
                if uid.is_none() {
                    eprintln!("chown: invalid user: '{}'", owner);
                    fatal = true;
                }
            }
            if !group.is_empty() {
                gid = get_gid(OsStr::new(group));
                if gid.is_none() {
                    eprintln!("chown: invalid group: '{}'", group);
                    fatal = true;
                }
            }
        } else {
            eprintln!("chown: invalid user: '{}'", lossy);
            fatal = true;
        }
    }

    if fatal {
        process::exit(1);
    }

    let mut exit_code = 0;
    for file in files {
        let path = Path::new(file);
        if let Err(e) = chown(path, uid, gid) {
            eprintln!("chown: changing ownership of '{}': {}", path.display(), e);
            exit_code = 1;
        }
    }

    if exit_code != 0 {
        process::exit(exit_code);
    }
}
