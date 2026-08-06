// src/bin/ls.rs
use std::collections::HashMap;
use std::env;
use std::ffi::CStr;
use std::fs::{self, Metadata};
use std::io::{self, IsTerminal, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = "ls (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: ls [OPTION]... [FILE]...\n\
List information about the FILEs (the current directory by default).\n\
\n\
  -a, --all                  do not ignore entries starting with .\n\
  -A, --almost-all           do not list implied . and ..\n\
  -C                         list entries by columns\n\
  -d, --directory            list directories themselves, not their contents\n\
  -F, --classify             append indicator (one of */=>@|) to entries\n\
  -h, --human-readable       with -l, print sizes in human readable format\n\
  -l                         use a long listing format\n\
  -r, --reverse              reverse order while sorting\n\
  -R, --recursive            list subdirectories recursively\n\
  -S                         sort by file size, largest first\n\
  -t                         sort by time, newest first\n\
  -1                         list one file per line\n\
      --color[=WHEN]         colorize the output\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

#[derive(PartialEq)]
enum SortBy {
    Name,
    Size,
    Time,
    None,
}

struct Options {
    all: bool,
    almost_all: bool,
    long: bool,
    human_readable: bool,
    recursive: bool,
    one_line: bool,
    columns: bool,
    classify: bool,
    color: bool,
    sort_by: SortBy,
    reverse: bool,
    dereference: bool,
    show_dir_itself: bool,
    paths: Vec<PathBuf>,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        all: false,
        almost_all: false,
        long: false,
        human_readable: false,
        recursive: false,
        one_line: false,
        columns: false,
        classify: false,
        color: false,
        sort_by: SortBy::Name,
        reverse: false,
        dereference: false,
        show_dir_itself: false,
        paths: Vec::new(),
    };

    let is_tty = io::stdout().is_terminal();
    if is_tty {
        opts.columns = true;
        opts.color = true;
    }

    let mut args = env::args().skip(1);
    let mut end_of_opts = false;

    while let Some(arg) = args.next() {
        if !end_of_opts && arg.starts_with('-') && arg.len() > 1 {
            if arg == "--" {
                end_of_opts = true;
                continue;
            }
            if arg == "--help" {
                println!("{}", HELP);
                std::process::exit(0);
            }
            if arg == "--version" {
                println!("{}", VERSION);
                std::process::exit(0);
            }

            if arg.starts_with("--color") {
                opts.color = if arg.contains("never") { false } else { true };
                continue;
            }

            for c in arg.chars().skip(1) {
                match c {
                    'a' => opts.all = true,
                    'A' => opts.almost_all = true,
                    'C' => {
                        opts.columns = true;
                        opts.one_line = false;
                    }
                    'd' => opts.show_dir_itself = true,
                    'F' => opts.classify = true,
                    'h' => opts.human_readable = true,
                    'l' => {
                        opts.long = true;
                        opts.columns = false;
                        opts.one_line = false;
                    }
                    'r' => opts.reverse = true,
                    'R' => opts.recursive = true,
                    'S' => opts.sort_by = SortBy::Size,
                    't' => opts.sort_by = SortBy::Time,
                    '1' => {
                        opts.one_line = true;
                        opts.columns = false;
                    }
                    'L' => opts.dereference = true,
                    'U' => opts.sort_by = SortBy::None,
                    _ => return Err(format!("invalid option -- '{}'", c)),
                }
            }
        } else {
            opts.paths.push(PathBuf::from(arg));
        }
    }

    if opts.paths.is_empty() {
        opts.paths.push(PathBuf::from("."));
    }
    Ok(opts)
}

struct Entry {
    name: String,
    path: PathBuf,
    metadata: Option<Metadata>,
    file_type: fs::FileType,
    link_target: Option<String>,
}

struct Cache {
    users: HashMap<u32, String>,
    groups: HashMap<u32, String>,
}

impl Cache {
    fn new() -> Self {
        Self {
            users: HashMap::new(),
            groups: HashMap::new(),
        }
    }

    fn get_user(&mut self, uid: u32) -> &str {
        if !self.users.contains_key(&uid) {
            let name = unsafe {
                let pwd = libc::getpwuid(uid);
                if !pwd.is_null() {
                    CStr::from_ptr((*pwd).pw_name)
                        .to_string_lossy()
                        .into_owned()
                } else {
                    uid.to_string()
                }
            };
            self.users.insert(uid, name);
        }
        self.users.get(&uid).unwrap()
    }

    fn get_group(&mut self, gid: u32) -> &str {
        if !self.groups.contains_key(&gid) {
            let name = unsafe {
                let grp = libc::getgrgid(gid);
                if !grp.is_null() {
                    CStr::from_ptr((*grp).gr_name)
                        .to_string_lossy()
                        .into_owned()
                } else {
                    gid.to_string()
                }
            };
            self.groups.insert(gid, name);
        }
        self.groups.get(&gid).unwrap()
    }
}

fn collect_entries(path: &Path, opts: &Options) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let needs_meta =
        opts.long || opts.sort_by == SortBy::Size || opts.sort_by == SortBy::Time || opts.classify;

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();

        if !opts.all && !opts.almost_all && name.starts_with('.') {
            continue;
        }
        if opts.almost_all && (name == "." || name == "..") {
            continue;
        }

        let ft = if opts.dereference {
            fs::metadata(entry.path())?.file_type()
        } else {
            entry.file_type()?
        };
        let meta = if needs_meta {
            Some(if opts.dereference {
                fs::metadata(entry.path())?
            } else {
                fs::symlink_metadata(entry.path())?
            })
        } else {
            None
        };
        let link_target = if ft.is_symlink() && opts.long {
            fs::read_link(entry.path())
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        } else {
            None
        };

        entries.push(Entry {
            name,
            path: entry.path(),
            metadata: meta,
            file_type: ft,
            link_target,
        });
    }

    match opts.sort_by {
        SortBy::Name => entries.sort_by(|a, b| a.name.cmp(&b.name)),
        SortBy::Size => entries.sort_by(|a, b| {
            b.metadata
                .as_ref()
                .unwrap()
                .len()
                .cmp(&a.metadata.as_ref().unwrap().len())
        }),
        SortBy::Time => entries.sort_by(|a, b| {
            b.metadata
                .as_ref()
                .unwrap()
                .mtime()
                .cmp(&a.metadata.as_ref().unwrap().mtime())
        }),
        SortBy::None => {}
    }
    if opts.reverse {
        entries.reverse();
    }

    Ok(entries)
}

fn color_for(e: &Entry) -> &'static str {
    if e.file_type.is_dir() {
        "\x1b[1;34m"
    } else if e.file_type.is_symlink() {
        "\x1b[1;36m"
    } else if e.file_type.is_fifo() {
        "\x1b[33m"
    } else if e.file_type.is_socket() {
        "\x1b[1;35m"
    } else if e.file_type.is_block_device() || e.file_type.is_char_device() {
        "\x1b[1;33m"
    } else if let Some(meta) = &e.metadata {
        if meta.mode() & 0o111 != 0 {
            "\x1b[1;32m"
        } else {
            ""
        }
    } else {
        ""
    }
}

fn classify_char(e: &Entry) -> &'static str {
    if e.file_type.is_dir() {
        "/"
    } else if e.file_type.is_symlink() {
        "@"
    } else if e.file_type.is_fifo() {
        "|"
    } else if e.file_type.is_socket() {
        "="
    } else if let Some(meta) = &e.metadata {
        if meta.mode() & 0o111 != 0 {
            "*"
        } else {
            ""
        }
    } else {
        ""
    }
}

struct FormattedName {
    text: String,
    visible_len: usize,
}

fn format_name(e: &Entry, opts: &Options) -> FormattedName {
    let mut text = String::new();
    let mut visible_len = e.name.len();
    let color = if opts.color { color_for(e) } else { "" };
    let reset = if opts.color && !color.is_empty() {
        "\x1b[0m"
    } else {
        ""
    };

    if !color.is_empty() {
        text.push_str(color);
    }
    text.push_str(&e.name);
    if !reset.is_empty() {
        text.push_str(reset);
    }

    if opts.classify {
        let c = classify_char(e);
        text.push_str(c);
        visible_len += c.len();
    }
    FormattedName { text, visible_len }
}

fn get_term_width() -> usize {
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
            ws.ws_col as usize
        } else {
            80
        }
    }
}

fn print_columns<W: Write>(
    w: &mut W,
    entries: &[Entry],
    term_width: usize,
    opts: &Options,
) -> io::Result<()> {
    let formatted: Vec<FormattedName> = entries.iter().map(|e| format_name(e, opts)).collect();
    let max_len = formatted.iter().map(|n| n.visible_len).max().unwrap_or(0);
    let col_width = max_len + 2;
    let cols = if term_width == 0 {
        1
    } else {
        std::cmp::max(1, term_width / col_width)
    };

    for chunk in formatted.chunks(cols) {
        for (i, name) in chunk.iter().enumerate() {
            if i == chunk.len() - 1 {
                writeln!(w, "{}", name.text)?;
            } else {
                write!(w, "{}", name.text)?;
                let padding = col_width - name.visible_len;
                for _ in 0..padding {
                    w.write_all(b" ")?;
                }
            }
        }
    }
    Ok(())
}

fn print_short<W: Write>(w: &mut W, entries: &[Entry], opts: &Options) -> io::Result<()> {
    for e in entries {
        let formatted = format_name(e, opts);
        writeln!(w, "{}", formatted.text)?;
    }
    Ok(())
}

fn human_readable(size: u64) -> String {
    let mut s = size as f64;
    let units = ["", "K", "M", "G", "T", "P"];
    let mut i = 0;
    while s >= 1024.0 && i < units.len() - 1 {
        s /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{}", size)
    } else {
        format!("{:.1}{}", s, units[i])
    }
}

fn format_perms(mode: u32) -> String {
    let mut s = String::with_capacity(10);
    s.push(match mode & libc::S_IFMT {
        libc::S_IFDIR => 'd',
        libc::S_IFLNK => 'l',
        libc::S_IFBLK => 'b',
        libc::S_IFCHR => 'c',
        libc::S_IFIFO => 'p',
        libc::S_IFSOCK => 's',
        _ => '-',
    });
    s.push(if mode & 0o400 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o200 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o100 != 0 {
        if mode & libc::S_ISUID != 0 {
            's'
        } else {
            'x'
        }
    } else {
        if mode & libc::S_ISUID != 0 {
            'S'
        } else {
            '-'
        }
    });
    s.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o010 != 0 {
        if mode & libc::S_ISGID != 0 {
            's'
        } else {
            'x'
        }
    } else {
        if mode & libc::S_ISGID != 0 {
            'S'
        } else {
            '-'
        }
    });
    s.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o001 != 0 {
        if mode & libc::S_ISVTX != 0 {
            't'
        } else {
            'x'
        }
    } else {
        if mode & libc::S_ISVTX != 0 {
            'T'
        } else {
            '-'
        }
    });
    s
}

fn format_time(mtime: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let six_months = 180 * 24 * 3600;
    let is_old = (now - mtime).abs() > six_months;
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    unsafe {
        libc::localtime_r(&mtime, &mut tm);
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
        String::from_utf8_lossy(&buf[..len]).into_owned()
    } else {
        "Jan  1  1970".to_string()
    }
}

fn print_long<W: Write>(
    w: &mut W,
    entries: &[Entry],
    opts: &Options,
    cache: &mut Cache,
) -> io::Result<()> {
    let mut max_links = 0;
    let mut max_user = 0;
    let mut max_group = 0;
    let mut max_size = 0;
    let mut rows = Vec::with_capacity(entries.len());
    let mut total_blocks = 0u64;

    for e in entries {
        let meta = e.metadata.as_ref().unwrap();
        total_blocks += meta.blocks() / 2;
        let links = meta.nlink();
        let user = cache.get_user(meta.uid()).to_string();
        let group = cache.get_group(meta.gid()).to_string();
        let size = meta.len();

        max_links = std::cmp::max(max_links, links.to_string().len());
        max_user = std::cmp::max(max_user, user.len());
        max_group = std::cmp::max(max_group, group.len());
        let size_str = if opts.human_readable {
            human_readable(size)
        } else {
            size.to_string()
        };
        max_size = std::cmp::max(max_size, size_str.len());

        rows.push((links, user, group, size_str, meta, e));
    }

    writeln!(w, "total {}", total_blocks)?;

    for (links, user, group, size_str, meta, e) in rows {
        let perms = format_perms(meta.mode());
        let time = format_time(meta.mtime());
        let formatted = format_name(e, opts);

        write!(
            w,
            "{} {:>links$} {:<user$} {:<group$} {:>size$} {} {}",
            perms,
            links,
            user,
            group,
            size_str,
            time,
            formatted.text,
            links = max_links,
            user = max_user,
            group = max_group,
            size = max_size
        )?;

        if e.file_type.is_symlink() {
            if let Some(target) = &e.link_target {
                write!(w, " -> {}", target)?;
            }
        }
        writeln!(w)?;
    }
    Ok(())
}

fn process_path<W: Write>(
    w: &mut W,
    path: &Path,
    opts: &Options,
    cache: &mut Cache,
    multiple: bool,
) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;

    if meta.is_dir() && !opts.show_dir_itself {
        if multiple {
            writeln!(w, "{}:", path.display())?;
        }
        let entries = collect_entries(path, opts)?;
        if opts.long {
            print_long(w, &entries, opts, cache)?;
        } else if opts.columns {
            print_columns(w, &entries, get_term_width(), opts)?;
        } else {
            print_short(w, &entries, opts)?;
        }

        if opts.recursive {
            for e in entries {
                if e.file_type.is_dir() && e.name != "." && e.name != ".." {
                    writeln!(w)?;
                    process_path(w, &e.path, opts, cache, true)?;
                }
            }
        }
    } else {
        let ft = if opts.dereference {
            fs::metadata(path)?.file_type()
        } else {
            meta.file_type()
        };
        let entries = vec![Entry {
            name: path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned(),
            path: path.to_path_buf(),
            metadata: Some(meta),
            file_type: ft,
            link_target: if ft.is_symlink() {
                fs::read_link(path)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
            } else {
                None
            },
        }];
        if opts.long {
            print_long(w, &entries, opts, cache)?;
        } else if opts.columns {
            print_columns(w, &entries, get_term_width(), opts)?;
        } else {
            print_short(w, &entries, opts)?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let arg0 = env::args_os()
        .next()
        .and_then(|s| {
            PathBuf::from(s)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_default();

    let mut opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("ls: {}", e);
            return ExitCode::from(1);
        }
    };

    match arg0.as_str() {
        "dir" => {
            opts.columns = true;
            opts.one_line = false;
            opts.long = false;
        }
        "vdir" => {
            opts.long = true;
            opts.columns = false;
            opts.one_line = false;
        }
        _ => {}
    }

    let stdout = io::stdout();
    let mut w = io::BufWriter::with_capacity(64 * 1024, stdout.lock());
    let mut cache = Cache::new();
    let mut had_error = false;
    let multiple = opts.paths.len() > 1;

    let mut sorted_paths = opts.paths.clone();
    sorted_paths.sort();

    for (i, path) in sorted_paths.iter().enumerate() {
        if i > 0 {
            writeln!(w).unwrap();
        }
        if let Err(e) = process_path(&mut w, path, &opts, &mut cache, multiple) {
            eprintln!("ls: cannot access '{}': {}", path.display(), e);
            had_error = true;
        }
    }

    if had_error {
        ExitCode::from(2)
    } else {
        ExitCode::from(0)
    }
}
