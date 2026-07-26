use std::env;
use std::ffi::CStr;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn format_permissions(mode: u32) -> String {
    let mut s = String::with_capacity(10);
    let file_type = match mode & libc::S_IFMT {
        libc::S_IFDIR => 'd',
        libc::S_IFLNK => 'l',
        libc::S_IFBLK => 'b',
        libc::S_IFCHR => 'c',
        libc::S_IFIFO => 'p',
        libc::S_IFSOCK => 's',
        _ => '-',
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

fn format_time(t: SystemTime) -> String {
    let now = SystemTime::now();
    let six_months = std::time::Duration::from_secs(6 * 30 * 24 * 60 * 60);

    let is_old = if let Ok(diff) = now.duration_since(t) {
        diff > six_months
    } else {
        false
    };

    if let Ok(duration) = t.duration_since(UNIX_EPOCH) {
        let secs = duration.as_secs();
        let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
        unsafe {
            libc::localtime_r(&(secs as libc::time_t), &mut tm);
        }
        let mut buf = [0u8; 64];
        let fmt: &[u8] = if is_old {
            b"%b %e  %Y\0"
        } else {
            b"%b %e %H:%M\0"
        };
        let len = unsafe {
            libc::strftime(
                buf.as_mut_ptr() as *mut libc::c_char,
                buf.len(),
                fmt.as_ptr() as *const libc::c_char,
                &tm,
            )
        };
        if len > 0 {
            return String::from_utf8_lossy(&buf[..len]).into_owned();
        }
    }
    if is_old {
        "Jan  1  1970".to_string()
    } else {
        "Jan  1 00:00".to_string()
    }
}

struct Entry {
    name: String,
    meta: fs::Metadata,
    target: Option<String>,
}

fn collect_entries(
    path: &Path,
    show_all: bool,
    show_dir_itself: bool,
) -> io::Result<(Vec<Entry>, bool)> {
    let mut entries = Vec::new();
    let meta = fs::symlink_metadata(path)?;
    let is_dir = meta.is_dir();
    let is_dir_content = is_dir && !show_dir_itself;

    if !is_dir || show_dir_itself {
        let target = if meta.file_type().is_symlink() {
            fs::read_link(path)
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        } else {
            None
        };
        entries.push(Entry {
            name: path
                .file_name()
                .unwrap_or_else(|| path.as_os_str())
                .to_string_lossy()
                .into_owned(),
            meta,
            target,
        });
    } else {
        if show_all {
            for dot in [".", ".."] {
                let dot_path = path.join(dot);
                let dot_meta = fs::symlink_metadata(&dot_path)?;
                entries.push(Entry {
                    name: dot.to_string(),
                    meta: dot_meta,
                    target: None,
                });
            }
        }

        for item in fs::read_dir(path)? {
            let item = item?;
            let name = item.file_name().to_string_lossy().into_owned();
            if !show_all && name.starts_with('.') {
                continue;
            }
            let meta = fs::symlink_metadata(item.path())?;
            let target = if meta.file_type().is_symlink() {
                fs::read_link(item.path())
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
            } else {
                None
            };
            entries.push(Entry { name, meta, target });
        }
    }

    entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    Ok((entries, is_dir_content))
}

fn list_long<W: Write>(
    w: &mut W,
    entries: &[Entry],
    show_name: bool,
    dir_name: &str,
    show_total: bool,
) -> io::Result<()> {
    if show_name {
        writeln!(w, "{}:", dir_name)?;
    }

    let mut total_blocks = 0u64;
    let mut row_data = Vec::new();

    let mut max_links = 0;
    let mut max_user = 0;
    let mut max_group = 0;
    let mut max_size_or_dev = 0;

    for entry in entries {
        total_blocks += entry.meta.blocks() / 2;

        let perms = format_permissions(entry.meta.mode());
        let links = entry.meta.nlink().to_string();
        let user = get_user_name(entry.meta.uid());
        let group = get_group_name(entry.meta.gid());

        let size_or_dev =
            if entry.meta.mode() & libc::S_IFBLK != 0 || entry.meta.mode() & libc::S_IFCHR != 0 {
                let major = libc::major(entry.meta.rdev() as libc::dev_t);
                let minor = libc::minor(entry.meta.rdev() as libc::dev_t);
                format!("{}, {}", major, minor)
            } else {
                entry.meta.len().to_string()
            };

        let time_str = format_time(entry.meta.modified().unwrap_or(UNIX_EPOCH));

        max_links = std::cmp::max(max_links, links.len());
        max_user = std::cmp::max(max_user, user.len());
        max_group = std::cmp::max(max_group, group.len());
        max_size_or_dev = std::cmp::max(max_size_or_dev, size_or_dev.len());

        row_data.push((perms, links, user, group, size_or_dev, time_str, entry));
    }

    if show_total {
        writeln!(w, "total {}", total_blocks)?;
    }
    for (perms, links, user, group, size_or_dev, time_str, entry) in row_data {
        let name = if let Some(ref target) = entry.target {
            format!("{} -> {}", entry.name, target)
        } else {
            entry.name.clone()
        };

        writeln!(
            w,
            "{} {:>links_w$} {:<user_w$} {:<group_w$} {:>size_w$} {} {}",
            perms,
            links,
            user,
            group,
            size_or_dev,
            time_str,
            name,
            links_w = max_links,
            user_w = max_user,
            group_w = max_group,
            size_w = max_size_or_dev
        )?;
    }

    Ok(())
}

fn list_short<W: Write>(w: &mut W, entries: &[Entry], dir_mode: bool) -> io::Result<()> {
    if dir_mode {
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        writeln!(w, "{}", names.join("  "))?;
    } else {
        for entry in entries {
            writeln!(w, "{}", entry.name)?;
        }
    }
    Ok(())
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
    let mut show_dir_itself = false;
    let mut paths: Vec<PathBuf> = Vec::new();

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') && s != "-" {
            for c in s.chars().skip(1) {
                match c {
                    'a' => show_all = true,
                    'l' => long_format = true,
                    'd' => show_dir_itself = true,
                    _ => {
                        eprintln!("ls: invalid option: -{}", c);
                        std::process::exit(1);
                    }
                }
            }
        } else {
            paths.push(PathBuf::from(arg));
        }
    }

    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }

    let multiple = paths.len() > 1;
    paths.sort_unstable();

    let dir_mode = arg0 == "dir";

    for (i, path) in paths.iter().enumerate() {
        if i > 0 {
            writeln!(stdout)?;
        }

        let (entries, is_dir_content) = match collect_entries(path, show_all, show_dir_itself) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("ls: cannot access '{}': {}", path.display(), e);
                continue;
            }
        };

        if long_format {
            list_long(
                &mut stdout,
                &entries,
                multiple,
                &path.to_string_lossy(),
                is_dir_content,
            )?;
        } else {
            if multiple && is_dir_content {
                writeln!(stdout, "{}:", path.display())?;
            }
            list_short(&mut stdout, &entries, dir_mode)?;
        }
    }

    Ok(())
}
