use std::env;
use std::ffi::CStr;
use std::path::Path;

fn get_user_name(uid: libc::uid_t) -> String {
    unsafe {
        let pwd = libc::getpwuid(uid);
        if !pwd.is_null() {
            CStr::from_ptr((*pwd).pw_name)
                .to_string_lossy()
                .into_owned()
        } else {
            uid.to_string()
        }
    }
}

fn get_group_name(gid: libc::gid_t) -> String {
    unsafe {
        let grp = libc::getgrgid(gid);
        if !grp.is_null() {
            CStr::from_ptr((*grp).gr_name)
                .to_string_lossy()
                .into_owned()
        } else {
            gid.to_string()
        }
    }
}

fn main() {
    let args: Vec<_> = env::args_os().collect();
    let arg0 = args
        .get(0)
        .map(|s| {
            Path::new(s)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
        })
        .unwrap_or("");

    if arg0 == "whoami" {
        let uid = unsafe { libc::getuid() };
        println!("{}", get_user_name(uid));
        return;
    }

    let mut print_uid = false;
    let mut print_gid = false;
    let mut print_name = false;

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') && s != "-" {
            for c in s.chars().skip(1) {
                match c {
                    'u' => print_uid = true,
                    'g' => print_gid = true,
                    'n' => print_name = true,
                    _ => {
                        eprintln!("id: invalid option: -{}", c);
                        std::process::exit(1);
                    }
                }
            }
        }
    }

    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    if print_uid {
        if print_name {
            println!("{}", get_user_name(uid));
        } else {
            println!("{}", uid);
        }
    } else if print_gid {
        if print_name {
            println!("{}", get_group_name(gid));
        } else {
            println!("{}", gid);
        }
    } else {
        let user = get_user_name(uid);
        let group = get_group_name(gid);
        println!("uid={}({}) gid={}({})", uid, user, gid, group);
    }
}
