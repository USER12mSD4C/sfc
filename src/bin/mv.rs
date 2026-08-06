// src/bin/mv.rs
use std::env;
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = "mv (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: mv [OPTION]... [-T] SOURCE DEST\n\
       or:  mv [OPTION]... SOURCE... DIRECTORY\n\
       or:  mv [OPTION]... -t DIRECTORY SOURCE...\n\
Rename SOURCE to DEST, or move SOURCE(s) to DIRECTORY.\n\
\n\
  -f, --force                  do not prompt before overwriting\n\
  -i, --interactive            prompt before overwrite\n\
  -n, --no-clobber             do not overwrite an existing file\n\
  -u, --update                 move only when the SOURCE is newer than DEST\n\
  -v, --verbose                explain what is being done\n\
  -t, --target-directory=DIRECTORY  move all SOURCE arguments into DIRECTORY\n\
  -T, --no-target-directory    treat DEST as a normal file\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

const RENAME_NOREPLACE: libc::c_uint = 1;
const FICLONE: libc::c_ulong = 0x40049409;

struct Options {
    force: bool,
    interactive: bool,
    no_clobber: bool,
    update: bool,
    verbose: bool,
    target_dir: Option<PathBuf>,
    no_target_dir: bool,
    sources: Vec<PathBuf>,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        force: false,
        interactive: false,
        no_clobber: false,
        update: false,
        verbose: false,
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

            for c in arg.chars().skip(1) {
                match c {
                    'f' => {
                        opts.force = true;
                        opts.interactive = false;
                        opts.no_clobber = false;
                    }
                    'i' => {
                        opts.interactive = true;
                        opts.force = false;
                        opts.no_clobber = false;
                    }
                    'n' => {
                        opts.no_clobber = true;
                        opts.force = false;
                        opts.interactive = false;
                    }
                    'u' => opts.update = true,
                    'v' => opts.verbose = true,
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

fn renameat2_noreplace(src: &Path, dest: &Path) -> io::Result<()> {
    let src_c = CString::new(src.to_string_lossy().as_bytes()).unwrap();
    let dest_c = CString::new(dest.to_string_lossy().as_bytes()).unwrap();
    let res = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            src_c.as_ptr(),
            libc::AT_FDCWD,
            dest_c.as_ptr(),
            RENAME_NOREPLACE,
        )
    };
    if res < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("mv: {}", e);
            eprintln!("Try 'mv --help' for more information.");
            return ExitCode::from(1);
        }
    };

    let dest: PathBuf;
    let sources: &[PathBuf];

    if let Some(ref t) = opts.target_dir {
        dest = t.clone();
        sources = &opts.sources;
    } else if opts.no_target_dir {
        if opts.sources.len() != 2 {
            eprintln!("mv: extra operand '{}'", opts.sources[2].display());
            return ExitCode::from(1);
        }
        dest = opts.sources[1].clone();
        sources = &opts.sources[..1];
    } else if opts.sources.len() == 2 {
        dest = opts.sources[1].clone();
        sources = &opts.sources[..1];
    } else if opts.sources.len() > 2 {
        let last = opts.sources.last().unwrap();
        if last.is_dir() {
            dest = last.clone();
            sources = &opts.sources[..opts.sources.len() - 1];
        } else {
            eprintln!("mv: target '{}' is not a directory", last.display());
            return ExitCode::from(1);
        }
    } else {
        eprintln!(
            "mv: missing destination file operand after '{}'",
            opts.sources[0].display()
        );
        return ExitCode::from(1);
    }

    let mut had_error = false;

    for src in sources {
        let final_dest = if dest.is_dir() && !opts.no_target_dir {
            dest.join(src.file_name().unwrap_or(src.as_os_str()))
        } else {
            dest.clone()
        };

        if let Ok(dest_meta) = fs::symlink_metadata(&final_dest) {
            if opts.no_clobber {
                continue;
            }
            if opts.update {
                if let Ok(src_meta) = fs::symlink_metadata(src) {
                    if src_meta.mtime() <= dest_meta.mtime() {
                        continue;
                    }
                }
            }
            if opts.interactive {
                eprint!("mv: overwrite '{}'? ", final_dest.display());
                let mut ans = String::new();
                io::stdin().read_line(&mut ans).unwrap_or(0);
                if !ans.starts_with('y') && !ans.starts_with('Y') {
                    continue;
                }
            }
        }

        let res = if opts.no_clobber {
            renameat2_noreplace(src, &final_dest)
        } else {
            fs::rename(src, &final_dest)
        };

        match res {
            Ok(_) => {
                if opts.verbose {
                    println!("renamed '{}' -> '{}'", src.display(), final_dest.display());
                }
            }
            Err(ref e) if e.raw_os_error() == Some(libc::EXDEV) => {
                if let Err(e) = move_cross_device(src, &final_dest) {
                    eprintln!(
                        "mv: cannot move '{}' to '{}': {}",
                        src.display(),
                        final_dest.display(),
                        e
                    );
                    had_error = true;
                } else if opts.verbose {
                    println!("'{}' -> '{}'", src.display(), final_dest.display());
                }
            }
            Err(e) => {
                eprintln!(
                    "mv: cannot move '{}' to '{}': {}",
                    src.display(),
                    final_dest.display(),
                    e
                );
                had_error = true;
            }
        }
    }

    if had_error {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}

fn move_cross_device(src: &Path, dest: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_dir() {
        fs::create_dir(dest)?;
        copy_dir_cross(src, dest)?;
        unsafe {
            libc::chmod(dest.to_str().unwrap().as_ptr() as *const i8, meta.mode());
            libc::chown(
                dest.to_str().unwrap().as_ptr() as *const i8,
                meta.uid(),
                meta.gid(),
            );
        }
        fs::remove_dir_all(src)?;
    } else if meta.file_type().is_symlink() {
        let target = fs::read_link(src)?;
        std::os::unix::fs::symlink(target, dest)?;
        fs::remove_file(src)?;
    } else {
        copy_file_cross(src, dest, &meta)?;
        fs::remove_file(src)?;
    }
    Ok(())
}

fn copy_dir_cross(src: &Path, dest: &Path) -> io::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        let meta = fs::symlink_metadata(&src_path)?;
        if meta.file_type().is_dir() {
            fs::create_dir(&dest_path)?;
            copy_dir_cross(&src_path, &dest_path)?;
        } else if meta.file_type().is_symlink() {
            let target = fs::read_link(&src_path)?;
            std::os::unix::fs::symlink(target, &dest_path)?;
        } else {
            copy_file_cross(&src_path, &dest_path, &meta)?;
        }
    }
    Ok(())
}

fn copy_file_cross(src: &Path, dest: &Path, meta: &fs::Metadata) -> io::Result<()> {
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

    let res = unsafe { libc::ioctl(dest_fd, FICLONE, src_fd) };
    if res == 0 {
        copied = true;
    }

    if !copied {
        let mut buf = [0u8; 128 * 1024];
        let mut src_file = src_file;
        loop {
            let n = src_file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            dest_file.write_all(&buf[..n])?;
        }
    }

    unsafe {
        libc::fchmod(dest_fd, meta.mode());
        libc::fchown(dest_fd, meta.uid(), meta.gid());
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
        libc::futimens(dest_fd, times.as_ptr());
    }
    Ok(())
}
