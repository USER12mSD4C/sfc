use std::env;
use std::ffi::CStr;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::UNIX_EPOCH;

fn format_permissions(mode: u32) -> String {
    let mut s = String::with_capacity(10);
    let file_type = if mode & libc::S_IFDIR != 0 {
        'd'
    } else if mode & libc::S_IFLNK != 0 {
        'l'
    } else if mode & libc::S_IFBLK != 0 {
        'b'
    } else if mode & libc::S_IFCHR != 0 {
        'c'
    } else if mode & libc::S_IFIFO != 0 {
        'p'
    } else {
        '-'
    };
    s.push(file_type);

    s.push(if mode & 0o400 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o200 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o100 != 0 { 'x' } else { '-' });
    s.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o010 != 0 { 'x' } else { '-' });
    s.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o001 != 0 { 'x' } else { '-' });
    s
}

fn get_user_name(uid: u32) -> String {
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

fn get_group_name(gid: u32) -> String {
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

fn format_time(t: std::time::SystemTime) -> String {
    if let Ok(duration) = t.duration_since(UNIX_EPOCH) {
        let secs = duration.as_secs();
        let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
        unsafe {
            libc::localtime_r(&(secs as libc::time_t), &mut tm);
        }
        let mut buf = [0u8; 64];
        let len = unsafe {
            libc::strftime(
                buf.as_mut_ptr() as *mut libc::c_char,
                buf.len(),
                b"%b %e %H:%M\0".as_ptr() as *const libc::c_char,
                &tm,
            )
        };
        if len > 0 {
            return String::from_utf8_lossy(&buf[..len]).into_owned();
        }
    }
    "Jan  1 00:00".to_string()
}

fn main() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
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

    let mut show_all = false;
    let mut long_format = arg0 == "vdir";
    let mut dir_path = std::ffi::OsStr::new(".");

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') && s != "-" {
            for c in s.chars().skip(1) {
                match c {
                    'a' => show_all = true,
                    'l' => long_format = true,
                    _ => {
                        eprintln!("ls: invalid option: -{}", c);
                        std::process::exit(1);
                    }
                }
            }
        } else {
            dir_path = arg;
        }
    }

    let mut entries = Vec::new();
    if show_all {
        entries.push(".".to_string());
        entries.push("..".to_string());
    }

    if let Ok(read_dir) = fs::read_dir(dir_path) {
        for entry in read_dir.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !show_all && name.starts_with('.') {
                continue;
            }
            entries.push(name);
        }
    }

    entries.sort_unstable();

    let parent_path = Path::new(dir_path);

    if long_format {
        // Подсчитываем суммарное количество занимаемых дисковых блоков (в КБ)
        let mut total_blocks = 0;
        let mut row_data = Vec::new();

        let mut max_links = 0;
        let mut max_user = 0;
        let mut max_group = 0;
        let mut max_size = 0;

        for name in entries {
            let full_path = parent_path.join(&name);
            if let Ok(meta) = fs::symlink_metadata(&full_path) {
                total_blocks += meta.blocks() / 2; // Переводим 512-байтовые блоки файловой системы в 1024-байтовые блоки ls

                let perms = format_permissions(meta.mode());
                let links = meta.nlink().to_string();
                let user = get_user_name(meta.uid());
                let group = get_group_name(meta.gid());
                let size = meta.len().to_string();
                let time_str = format_time(meta.modified().unwrap_or(UNIX_EPOCH));

                max_links = std::cmp::max(max_links, links.len());
                max_user = std::cmp::max(max_user, user.len());
                max_group = std::cmp::max(max_group, group.len());
                max_size = std::cmp::max(max_size, size.len());

                row_data.push((perms, links, user, group, size, time_str, name));
            }
        }

        writeln!(stdout, "total {}", total_blocks)?;
        for (perms, links, user, group, size, time_str, name) in row_data {
            writeln!(
                stdout,
                "{} {:>links_w$} {:<user_w$} {:<group_w$} {:>size_w$} {} {}",
                perms,
                links,
                user,
                group,
                size,
                time_str,
                name,
                links_w = max_links,
                user_w = max_user,
                group_w = max_group,
                size_w = max_size
            )?;
        }
    } else if arg0 == "dir" {
        // dir по стандарту выводит список горизонтально через два пробела
        writeln!(stdout, "{}", entries.join("  "))?;
    } else {
        for name in entries {
            writeln!(stdout, "{}", name)?;
        }
    }

    Ok(())
}
