// src/bin/cp.rs
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = "cp (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: cp [OPTION]... [-T] SOURCE DEST\n\
       or:  cp [OPTION]... SOURCE... DIRECTORY\n\
       or:  cp [OPTION]... -t DIRECTORY SOURCE...\n\
Copy SOURCE to DEST, or multiple SOURCE(s) to DIRECTORY.\n\
\n\
  -a, --archive                same as -dR --preserve=all\n\
  -d                           same as --no-dereference --preserve=links\n\
  -f, --force                  remove existing destination files\n\
  -i, --interactive            prompt before overwrite\n\
  -n, --no-clobber             do not overwrite an existing file\n\
  -p                           same as --preserve=mode,ownership,timestamps\n\
      --preserve[=ATTR_LIST]   preserve mode,ownership,timestamps,links,all\n\
  -R, -r, --recursive          copy directories recursively\n\
      --reflink[=WHEN]         control clone/CoW copies (auto,always,never)\n\
      --sparse[=WHEN]          control creation of sparse files\n\
  -v, --verbose                explain what is being done\n\
  -t, --target-directory=DIRECTORY  copy all SOURCE arguments into DIRECTORY\n\
  -T, --no-target-directory    treat DEST as a normal file\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

const FICLONE: libc::c_ulong = 0x40049409;

#[derive(Clone, Copy, PartialEq)]
enum When {
    Auto,
    Always,
    Never,
}

struct Options {
    recursive: bool,
    force: bool,
    interactive: bool,
    no_clobber: bool,
    verbose: bool,
    dereference: bool,
    preserve_mode: bool,
    preserve_ownership: bool,
    preserve_timestamps: bool,
    preserve_links: bool,
    reflink: When,
    sparse: When,
    target_dir: Option<PathBuf>,
    no_target_dir: bool,
    sources: Vec<PathBuf>,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        recursive: false,
        force: false,
        interactive: false,
        no_clobber: false,
        verbose: false,
        dereference: false,
        preserve_mode: false,
        preserve_ownership: false,
        preserve_timestamps: false,
        preserve_links: false,
        reflink: When::Never,
        sparse: When::Auto,
        target_dir: None,
        no_target_dir: false,
        sources: Vec::new(),
    };

    let mut args = env::args().skip(1).peekable();
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

            if arg.starts_with("--preserve=") {
                let attrs = &arg[11..];
                for attr in attrs.split(',') {
                    match attr {
                        "mode" => opts.preserve_mode = true,
                        "ownership" => opts.preserve_ownership = true,
                        "timestamps" => opts.preserve_timestamps = true,
                        "links" => opts.preserve_links = true,
                        "all" => {
                            opts.preserve_mode = true;
                            opts.preserve_ownership = true;
                            opts.preserve_timestamps = true;
                            opts.preserve_links = true;
                        }
                        _ => return Err(format!("invalid attribute: {}", attr)),
                    }
                }
                continue;
            }
            if arg == "--preserve" {
                opts.preserve_mode = true;
                opts.preserve_ownership = true;
                opts.preserve_timestamps = true;
                continue;
            }
            if arg.starts_with("--reflink=") {
                opts.reflink = match &arg[10..] {
                    "auto" => When::Auto,
                    "always" => When::Always,
                    "never" => When::Never,
                    _ => return Err("invalid reflink option".into()),
                };
                continue;
            }
            if arg == "--reflink" {
                opts.reflink = When::Auto;
                continue;
            }
            if arg.starts_with("--sparse=") {
                opts.sparse = match &arg[9..] {
                    "auto" => When::Auto,
                    "always" => When::Always,
                    "never" => When::Never,
                    _ => return Err("invalid sparse option".into()),
                };
                continue;
            }
            if arg.starts_with("-t") {
                let dir = if arg.len() > 2 {
                    arg[2..].to_string()
                } else {
                    args.next().ok_or("-t requires argument")?
                };
                opts.target_dir = Some(PathBuf::from(dir));
                continue;
            }
            if arg == "--target-directory" {
                opts.target_dir = Some(PathBuf::from(
                    args.next().ok_or("--target-directory requires argument")?,
                ));
                continue;
            }

            if arg == "--no-target-directory" || arg == "-T" {
                opts.no_target_dir = true;
                continue;
            }

            for c in arg.chars().skip(1) {
                match c {
                    'a' => {
                        opts.recursive = true;
                        opts.preserve_links = true;
                        opts.dereference = false;
                        opts.preserve_mode = true;
                        opts.preserve_ownership = true;
                        opts.preserve_timestamps = true;
                    }
                    'd' => {
                        opts.preserve_links = true;
                        opts.dereference = false;
                    }
                    'f' => opts.force = true,
                    'i' => opts.interactive = true,
                    'n' => opts.no_clobber = true,
                    'p' => {
                        opts.preserve_mode = true;
                        opts.preserve_ownership = true;
                        opts.preserve_timestamps = true;
                    }
                    'r' | 'R' => opts.recursive = true,
                    'v' => opts.verbose = true,
                    'L' => opts.dereference = true,
                    'P' => opts.dereference = false,
                    'T' => opts.no_target_dir = true,
                    _ => return Err(format!("invalid option -- '{}'", c)),
                }
            }
        } else {
            opts.sources.push(PathBuf::from(arg));
        }
    }

    if opts.sources.is_empty() {
        return Err("missing file operand".into());
    }

    Ok(opts)
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("cp: {}", e);
            eprintln!("Try 'cp --help' for more information.");
            return ExitCode::from(1);
        }
    };

    let dest: PathBuf;
    let sources: &[PathBuf];

    if let Some(ref t) = opts.target_dir {
        dest = t.clone();
        sources = &opts.sources;
    } else if opts.sources.len() > 1 && !opts.no_target_dir {
        let last = opts.sources.last().unwrap();
        if last.is_dir() {
            dest = last.clone();
            sources = &opts.sources[..opts.sources.len() - 1];
        } else {
            eprintln!("cp: target '{}' is not a directory", last.display());
            return ExitCode::from(1);
        }
    } else if opts.sources.len() == 1 {
        eprintln!(
            "cp: missing destination file operand after '{}'",
            opts.sources[0].display()
        );
        return ExitCode::from(1);
    } else {
        dest = opts.sources.last().unwrap().clone();
        sources = &opts.sources[..opts.sources.len() - 1];
    }

    let mut had_error = false;

    for src in sources {
        let final_dest = if dest.is_dir() && !opts.no_target_dir {
            dest.join(src.file_name().unwrap_or(src.as_os_str()))
        } else {
            dest.clone()
        };

        let meta = if opts.dereference {
            fs::metadata(src)
        } else {
            fs::symlink_metadata(src)
        };

        let meta = match meta {
            Ok(m) => m,
            Err(e) => {
                eprintln!("cp: cannot stat '{}': {}", src.display(), e);
                had_error = true;
                continue;
            }
        };

        let res = if meta.file_type().is_symlink() && !opts.dereference && opts.preserve_links {
            copy_symlink(src, &final_dest, &opts)
        } else if meta.is_dir() {
            if !opts.recursive {
                eprintln!(
                    "cp: -r not specified; omitting directory '{}'",
                    src.display()
                );
                had_error = true;
                continue;
            }
            copy_dir(src, &final_dest, &opts)
        } else {
            copy_file(src, &final_dest, &meta, &opts)
        };

        if let Err(e) = res {
            eprintln!(
                "cp: cannot copy '{}' to '{}': {}",
                src.display(),
                final_dest.display(),
                e
            );
            had_error = true;
        }
    }

    if had_error {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}

fn copy_symlink(src: &Path, dest: &Path, opts: &Options) -> io::Result<()> {
    let target = fs::read_link(src)?;
    if dest.exists() || dest.symlink_metadata().is_ok() {
        if opts.no_clobber {
            return Ok(());
        }
        if opts.interactive {
            eprint!("cp: overwrite '{}'? ", dest.display());
            let mut ans = String::new();
            io::stdin().read_line(&mut ans)?;
            if !ans.starts_with('y') && !ans.starts_with('Y') {
                return Ok(());
            }
        }
        if opts.force {
            let _ = fs::remove_file(dest);
        }
    }
    std::os::unix::fs::symlink(target, dest)?;
    if opts.verbose {
        println!("'{}' -> '{}'", src.display(), dest.display());
    }
    Ok(())
}

fn copy_file(src: &Path, dest: &Path, meta: &fs::Metadata, opts: &Options) -> io::Result<()> {
    if let Ok(dest_meta) = fs::symlink_metadata(dest) {
        if dest_meta.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "cannot overwrite directory with file",
            ));
        }
        if opts.no_clobber {
            return Ok(());
        }
        if opts.interactive {
            eprint!("cp: overwrite '{}'? ", dest.display());
            let mut ans = String::new();
            io::stdin().read_line(&mut ans)?;
            if !ans.starts_with('y') && !ans.starts_with('Y') {
                return Ok(());
            }
        }
        if opts.force {
            let _ = fs::remove_file(dest);
        }
    }

    let src_file = File::open(src)?;
    let mut dest_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(meta.mode())
        .open(dest)?;

    let src_fd = src_file.as_raw_fd();
    let dest_fd = dest_file.as_raw_fd();

    let mut copied = false;

    if opts.reflink != When::Never {
        let res = unsafe { libc::ioctl(dest_fd, FICLONE, src_fd) };
        if res == 0 {
            copied = true;
        } else if opts.reflink == When::Always {
            return Err(io::Error::new(io::ErrorKind::Other, "reflink failed"));
        }
    }

    if !copied {
        let mut offset: i64 = 0;
        let mut fallback = false;

        loop {
            let res = unsafe {
                libc::copy_file_range(
                    src_fd,
                    &mut offset,
                    dest_fd,
                    std::ptr::null_mut(),
                    1024 * 1024,
                    0,
                )
            };
            if res < 0 {
                let err = io::Error::last_os_error();
                if offset == 0
                    && (err.raw_os_error() == Some(libc::ENOSYS)
                        || err.raw_os_error() == Some(libc::EXDEV))
                {
                    fallback = true;
                    break;
                }
                return Err(err);
            } else if res == 0 {
                break;
            }
        }

        if fallback {
            let mut buf = [0u8; 128 * 1024];
            let mut written: u64 = 0;
            let mut src_file = src_file;
            loop {
                let n = src_file.read(&mut buf)?;
                if n == 0 {
                    break;
                }

                let do_sparse =
                    opts.sparse == When::Always || (opts.sparse == When::Auto && n == buf.len());
                if do_sparse && buf[..n].iter().all(|&b| b == 0) {
                    unsafe {
                        libc::lseek(dest_fd, n as i64, libc::SEEK_CUR);
                    }
                } else {
                    dest_file.write_all(&buf[..n])?;
                }
                written += n as u64;
            }
            if written < meta.len() {
                unsafe {
                    libc::ftruncate(dest_fd, meta.len() as i64);
                }
            }
        }
    }

    if opts.preserve_mode {
        unsafe {
            libc::fchmod(dest_fd, meta.mode());
        }
    }
    if opts.preserve_ownership {
        unsafe {
            libc::fchown(dest_fd, meta.uid(), meta.gid());
        }
    }
    if opts.preserve_timestamps {
        let times = [
            libc::timespec {
                tv_sec: meta.atime(),
                tv_nsec: meta.atime_nsec(),
            },
            libc::timespec {
                tv_sec: meta.mtime(),
                tv_nsec: meta.mtime_nsec(),
            },
        ];
        unsafe {
            libc::futimens(dest_fd, times.as_ptr());
        }
    }

    if opts.verbose {
        println!("'{}' -> '{}'", src.display(), dest.display());
    }
    Ok(())
}

fn copy_dir(src: &Path, dest: &Path, opts: &Options) -> io::Result<()> {
    let src_meta = fs::symlink_metadata(src)?;

    if let Ok(dest_meta) = fs::symlink_metadata(dest) {
        if !dest_meta.is_dir() {
            if opts.no_clobber {
                return Ok(());
            }
            if opts.force {
                let _ = fs::remove_file(dest);
            }
        }
    }

    fs::create_dir(dest)?;
    unsafe {
        libc::chmod(
            dest.to_str().unwrap().as_ptr() as *const i8,
            src_meta.mode() | 0o700,
        );
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        let meta = fs::symlink_metadata(&src_path)?;

        if meta.file_type().is_symlink() && !opts.dereference && opts.preserve_links {
            copy_symlink(&src_path, &dest_path, opts)?;
        } else if meta.is_dir() {
            copy_dir(&src_path, &dest_path, opts)?;
        } else {
            copy_file(&src_path, &dest_path, &meta, opts)?;
        }
    }

    if opts.preserve_mode {
        unsafe {
            libc::chmod(
                dest.to_str().unwrap().as_ptr() as *const i8,
                src_meta.mode(),
            );
        }
    }
    if opts.preserve_ownership {
        unsafe {
            libc::chown(
                dest.to_str().unwrap().as_ptr() as *const i8,
                src_meta.uid(),
                src_meta.gid(),
            );
        }
    }
    if opts.preserve_timestamps {
        let times = [
            libc::timespec {
                tv_sec: src_meta.atime(),
                tv_nsec: src_meta.atime_nsec(),
            },
            libc::timespec {
                tv_sec: src_meta.mtime(),
                tv_nsec: src_meta.mtime_nsec(),
            },
        ];
        unsafe {
            libc::utimensat(
                libc::AT_FDCWD,
                dest.to_str().unwrap().as_ptr() as *const i8,
                times.as_ptr(),
                0,
            );
        }
    }

    Ok(())
}
