// src/touch_impl.rs
use std::env;
use std::ffi::CString;
use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::AsRawFd;
use std::process::ExitCode;
use std::time::SystemTime;

const VERSION: &str = "touch (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: touch [OPTION]... FILE...\n\
Update the access and modification times of each FILE to the current time.\n\
\n\
  -a                     change only the access time\n\
  -c, --no-create        do not create any files\n\
  -d, --date=STRING      parse STRING and use it instead of current time\n\
  -h, --no-dereference   affect each symbolic link instead of any referenced file\n\
  -m                     change only the modification time\n\
  -r, --reference=FILE   use this file's times instead of current time\n\
  -t STAMP               use [[CC]YY]MMDDhhmm[.ss] instead of current time\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

struct Options {
    access: bool,
    modification: bool,
    no_create: bool,
    no_dereference: bool,
    reference: Option<String>,
    timestamp: Option<String>,
    date_string: Option<String>,
    files: Vec<String>,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        access: false,
        modification: false,
        no_create: false,
        no_dereference: false,
        reference: None,
        timestamp: None,
        date_string: None,
        files: Vec::new(),
    };

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

            if arg.starts_with("--date=") {
                opts.date_string = Some(arg[7..].to_string());
                continue;
            }
            if arg == "--no-create" {
                opts.no_create = true;
                continue;
            }
            if arg == "--no-dereference" {
                opts.no_dereference = true;
                continue;
            }
            if arg == "--reference" {
                opts.reference = Some(args.next().ok_or("--reference requires argument")?);
                continue;
            }
            if arg.starts_with("--reference=") {
                opts.reference = Some(arg[12..].to_string());
                continue;
            }

            if arg == "-r" {
                opts.reference = Some(args.next().ok_or("-r requires argument")?);
                continue;
            }
            if arg == "-t" {
                opts.timestamp = Some(args.next().ok_or("-t requires argument")?);
                continue;
            }

            for c in arg.chars().skip(1) {
                match c {
                    'a' => {
                        opts.access = true;
                        opts.modification = false;
                    }
                    'm' => {
                        opts.modification = true;
                        opts.access = false;
                    }
                    'c' => opts.no_create = true,
                    'h' => opts.no_dereference = true,
                    _ => return Err(format!("invalid option -- '{}'", c)),
                }
            }
        } else {
            opts.files.push(arg);
        }
    }

    if opts.files.is_empty() {
        return Err("missing file operand".into());
    }
    Ok(opts)
}

fn parse_timestamp(s: &str) -> Result<i64, String> {
    let (datetime_part, sec_part) = if let Some(dot) = s.find('.') {
        (&s[..dot], &s[dot + 1..])
    } else {
        (s, "0")
    };

    let len = datetime_part.len();
    if len < 8 || len > 12 {
        return Err("invalid date format".into());
    }

    let (year, month, day, hour, minute) = match len {
        8 => (
            None,
            &datetime_part[0..2],
            &datetime_part[2..4],
            &datetime_part[4..6],
            &datetime_part[6..8],
        ),
        10 => (
            Some(&datetime_part[0..2]),
            &datetime_part[2..4],
            &datetime_part[4..6],
            &datetime_part[6..8],
            &datetime_part[8..10],
        ),
        12 => (
            Some(&datetime_part[0..4]),
            &datetime_part[4..6],
            &datetime_part[6..8],
            &datetime_part[8..10],
            &datetime_part[10..12],
        ),
        _ => return Err("invalid date format".into()),
    };

    let month: i32 = month.parse().map_err(|_| "invalid month")?;
    let day: i32 = day.parse().map_err(|_| "invalid day")?;
    let hour: i32 = hour.parse().map_err(|_| "invalid hour")?;
    let minute: i32 = minute.parse().map_err(|_| "invalid minute")?;
    let sec: i32 = sec_part.parse().map_err(|_| "invalid seconds")?;

    let year = if let Some(y) = year {
        y.parse().map_err(|_| "invalid year")?
    } else {
        let now = SystemTime::now();
        let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap();
        let secs = duration.as_secs();
        let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
        unsafe {
            libc::localtime_r(&(secs as libc::time_t), &mut tm);
        }
        tm.tm_year + 1900
    };

    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    tm.tm_sec = sec;
    tm.tm_min = minute;
    tm.tm_hour = hour;
    tm.tm_mday = day;
    tm.tm_mon = month - 1;
    tm.tm_year = year - 1900;
    tm.tm_isdst = -1;

    let timestamp = unsafe { libc::mktime(&mut tm) };
    if timestamp == -1 {
        return Err("invalid date".into());
    }
    Ok(timestamp as i64)
}

fn parse_date_string(s: &str) -> Result<i64, String> {
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
        "%d %b %Y %H:%M:%S",
        "%d %b %Y %H:%M",
        "%d %b %Y",
    ];

    let c_str = CString::new(s).unwrap();
    for fmt in &formats {
        let c_fmt = CString::new(*fmt).unwrap();
        let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
        let res = unsafe { libc::strptime(c_str.as_ptr(), c_fmt.as_ptr(), &mut tm) };
        if !res.is_null() {
            let timestamp = unsafe { libc::mktime(&mut tm) };
            if timestamp != -1 {
                return Ok(timestamp as i64);
            }
        }
    }
    Err("invalid date".into())
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("touch: {}", e);
            eprintln!("Try 'touch --help' for more information.");
            return ExitCode::from(1);
        }
    };

    let (atime, mtime) = if let Some(ref r) = opts.reference {
        match fs::metadata(r) {
            Ok(meta) => (meta.atime(), meta.mtime()),
            Err(e) => {
                eprintln!("touch: failed to get attributes of '{}': {}", r, e);
                return ExitCode::from(1);
            }
        }
    } else if let Some(ref t) = opts.timestamp {
        match parse_timestamp(t) {
            Ok(ts) => (ts, ts),
            Err(e) => {
                eprintln!("touch: {}", e);
                return ExitCode::from(1);
            }
        }
    } else if let Some(ref d) = opts.date_string {
        match parse_date_string(d) {
            Ok(ts) => (ts, ts),
            Err(e) => {
                eprintln!("touch: {}", e);
                return ExitCode::from(1);
            }
        }
    } else {
        let now = SystemTime::now();
        let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap();
        let secs = duration.as_secs() as i64;
        (secs, secs)
    };

    let mut had_error = false;

    for file in &opts.files {
        let meta = if opts.no_dereference {
            fs::symlink_metadata(file)
        } else {
            fs::metadata(file)
        };

        let should_create = !opts.no_create;

        if meta.is_err() && !should_create {
            continue;
        }

        let res = if let Ok(m) = meta {
            let new_atime = if opts.modification && !opts.access {
                m.atime()
            } else {
                atime
            };
            let new_mtime = if opts.access && !opts.modification {
                m.mtime()
            } else {
                mtime
            };

            let times = [
                libc::timespec {
                    tv_sec: new_atime,
                    tv_nsec: 0,
                },
                libc::timespec {
                    tv_sec: new_mtime,
                    tv_nsec: 0,
                },
            ];
            if opts.no_dereference {
                unsafe {
                    libc::utimensat(
                        libc::AT_FDCWD,
                        file.as_ptr() as *const i8,
                        times.as_ptr(),
                        libc::AT_SYMLINK_NOFOLLOW,
                    )
                }
            } else {
                unsafe {
                    libc::utimensat(
                        libc::AT_FDCWD,
                        file.as_ptr() as *const i8,
                        times.as_ptr(),
                        0,
                    )
                }
            }
        } else {
            let file_res = OpenOptions::new().write(true).create(true).open(file);
            match file_res {
                Ok(f) => {
                    let times = [
                        libc::timespec {
                            tv_sec: atime,
                            tv_nsec: 0,
                        },
                        libc::timespec {
                            tv_sec: mtime,
                            tv_nsec: 0,
                        },
                    ];
                    unsafe { libc::futimens(f.as_raw_fd(), times.as_ptr()) }
                }
                Err(e) => {
                    eprintln!("touch: cannot touch '{}': {}", file, e);
                    had_error = true;
                    continue;
                }
            }
        };

        if res < 0 {
            let err = io::Error::last_os_error();
            eprintln!("touch: setting times of '{}': {}", file, err);
            had_error = true;
        }
    }

    if had_error {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}
