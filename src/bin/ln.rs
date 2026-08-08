use std::env;
use std::ffi::CString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

const VERSION: &str = "ln (sfc coreutils) 0.1.0";

const HELP: &str = "Usage: ln [OPTION]... [-T] TARGET LINK_NAME
       or:  ln [OPTION]... TARGET... DIRECTORY
       or:  ln [OPTION]... -t DIRECTORY TARGET...
Create links between files.

  -b, --backup                 make a backup of each existing destination file
  -d, --directory              allow the superuser to attempt hard links to directories
  -f, --force                  remove existing destination files
  -i, --interactive            prompt whether to remove existing destination files
  -L, --logical                dereference TARGET if it is a symbolic link
  -n, --no-dereference         treat LINK_NAME as a normal file if it is a symbolic link to a directory
  -P, --physical               create hard links directly to symbolic links
  -r, --relative               create symbolic links relative to the link location
  -s, --symbolic               create symbolic links instead of hard links
  -t, --target-directory=DIRECTORY
                               specify the DIRECTORY in which to create the links
  -T, --no-target-directory    treat LINK_NAME as a normal file always
  -v, --verbose                print the name of each linked file
      --help                   display this help and exit
      --version                output version information and exit";

#[derive(Default)]
struct Options {
    backup: bool,
    directory: bool,
    force: bool,
    interactive: bool,
    logical: bool,
    no_deref: bool,
    physical: bool,
    relative: bool,
    symbolic: bool,
    no_target_dir: bool,
    verbose: bool,
    target_dir: Option<PathBuf>,
    operands: Vec<PathBuf>,
}

enum Preparation {
    Create,
    Skip,
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("ln: {}", e);
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32, String> {
    let opts = parse_args()?;

    if opts.relative && !opts.symbolic {
        return Err("--relative can only be used with --symbolic".to_string());
    }

    if opts.target_dir.is_some() && opts.no_target_dir {
        return Err("cannot combine --target-directory and --no-target-directory".to_string());
    }

    if opts.no_target_dir && opts.operands.len() > 2 {
        return Err("extra operand after --no-target-directory".to_string());
    }

    if opts.operands.is_empty() {
        return Err("missing operand".to_string());
    }

    let mut had_error = false;

    if let Some(ref target_dir) = opts.target_dir {
        if opts.operands.is_empty() {
            return Err("missing operand".to_string());
        }

        if !is_directory_follow(target_dir) {
            return Err(format!("target '{}': not a directory", target_dir.display()));
        }

        for source in &opts.operands {
            let link = target_dir.join(file_name_of(source));
            if let Err(e) = create_link(source, &link, &opts) {
                eprintln!("ln: {}", e);
                had_error = true;
            }
        }
    } else if opts.operands.len() == 1 {
        let source = &opts.operands[0];
        let link = PathBuf::from(file_name_of(source));

        if let Err(e) = create_link(source, &link, &opts) {
            eprintln!("ln: {}", e);
            had_error = true;
        }
    } else if opts.operands.len() == 2 {
        let source = &opts.operands[0];
        let dest = &opts.operands[1];

        let mut dest_is_dir = false;

        if !opts.no_target_dir {
            dest_is_dir = is_existing_directory(dest, !opts.no_deref);

            if dest_is_dir
                && opts.symbolic
                && is_symlink_path(dest)
                && !has_trailing_slash(dest)
            {
                dest_is_dir = false;
            }
        }

        if dest_is_dir {
            let link = dest.join(file_name_of(source));
            if let Err(e) = create_link(source, &link, &opts) {
                eprintln!("ln: {}", e);
                had_error = true;
            }
        } else {
            if let Err(e) = create_link(source, dest, &opts) {
                eprintln!("ln: {}", e);
                had_error = true;
            }
        }
    } else {
        let last = opts.operands.last().unwrap();

        if opts.no_target_dir || !is_existing_directory(last, !opts.no_deref) {
            return Err(format!("target '{}': not a directory", last.display()));
        }

        for source in &opts.operands[..opts.operands.len() - 1] {
            let link = last.join(file_name_of(source));
            if let Err(e) = create_link(source, &link, &opts) {
                eprintln!("ln: {}", e);
                had_error = true;
            }
        }
    }

    if had_error {
        Ok(1)
    } else {
        Ok(0)
    }
}

fn parse_args() -> Result<Options, String> {
    let args: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    let mut opts = Options::default();
    let mut end_of_options = false;
    let mut i = 0;

    while i < args.len() {
        let arg = args[i].clone();
        let s = arg.to_string_lossy().into_owned();
        i += 1;

        if !end_of_options && s.starts_with('-') && s != "-" {
            if s == "--" {
                end_of_options = true;
                continue;
            }

            if s == "--help" {
                println!("{}", HELP);
                std::process::exit(0);
            }

            if s == "--version" {
                println!("{}", VERSION);
                std::process::exit(0);
            }

            if s == "--backup" {
                opts.backup = true;
                continue;
            }

            if let Some(_control) = s.strip_prefix("--backup=") {
                opts.backup = true;
                continue;
            }

            if s == "--directory" {
                opts.directory = true;
                continue;
            }

            if s == "--force" {
                opts.force = true;
                continue;
            }

            if s == "--interactive" {
                opts.interactive = true;
                continue;
            }

            if s == "--logical" {
                opts.logical = true;
                opts.physical = false;
                continue;
            }

            if s == "--no-dereference" {
                opts.no_deref = true;
                continue;
            }

            if s == "--physical" {
                opts.physical = true;
                opts.logical = false;
                continue;
            }

            if s == "--relative" {
                opts.relative = true;
                continue;
            }

            if s == "--symbolic" {
                opts.symbolic = true;
                continue;
            }

            if s == "--target-directory" {
                if i >= args.len() {
                    return Err("--target-directory requires an argument".to_string());
                }

                opts.target_dir = Some(args[i].clone());
                i += 1;
                continue;
            }

            if let Some(dir) = s.strip_prefix("--target-directory=") {
                opts.target_dir = Some(PathBuf::from(dir));
                continue;
            }

            if s == "--no-target-directory" {
                opts.no_target_dir = true;
                continue;
            }

            if s == "--verbose" {
                opts.verbose = true;
                continue;
            }

            if s.starts_with("--") {
                return Err(format!("unknown option: {}", s));
            }

            let mut chars = s.chars().skip(1).peekable();

            while let Some(c) = chars.next() {
                match c {
                    'b' => opts.backup = true,
                    'd' => opts.directory = true,
                    'f' => opts.force = true,
                    'i' => opts.interactive = true,
                    'L' => {
                        opts.logical = true;
                        opts.physical = false;
                    }
                    'n' => opts.no_deref = true,
                    'P' => {
                        opts.physical = true;
                        opts.logical = false;
                    }
                    'r' => opts.relative = true,
                    's' => opts.symbolic = true,
                    'T' => opts.no_target_dir = true,
                    'v' => opts.verbose = true,
                    't' => {
                        let rest: String = chars.collect();

                        let dir = if !rest.is_empty() {
                            PathBuf::from(rest)
                        } else {
                            if i >= args.len() {
                                return Err("-t requires an argument".to_string());
                            }

                            let dir = args[i].clone();
                            i += 1;
                            dir
                        };

                        opts.target_dir = Some(dir);
                        break;
                    }
                    _ => return Err(format!("invalid option -- '{}'", c)),
                }
            }
        } else {
            opts.operands.push(arg);
        }
    }

    Ok(opts)
}

fn create_link(source: &Path, dest: &Path, opts: &Options) -> Result<(), String> {
    let target_text = make_symlink_target_text(source, dest, opts.relative)
        .map_err(|e| format!("{}: {}", dest.display(), e))?;

    match prepare_destination(dest, opts, &target_text) {
        Ok(Preparation::Skip) => Ok(()),
        Ok(Preparation::Create) => {
            if opts.symbolic {
                symlink(&target_text, dest).map_err(|e| {
                    format!(
                        "failed to create symbolic link '{}' to '{}': {}",
                        dest.display(),
                        target_text.display(),
                        e
                    )
                })?;

                if opts.verbose {
                    println!("'{}' -> '{}'", dest.display(), target_text.display());
                }
            } else {
                hard_link_with_mode(source, dest, !opts.physical).map_err(|e| {
                    format!(
                        "failed to create hard link '{}' to '{}': {}",
                        dest.display(),
                        source.display(),
                        e
                    )
                })?;

                if opts.verbose {
                    println!("'{}' => '{}'", dest.display(), source.display());
                }
            }

            Ok(())
        }
        Err(e) => Err(format!("{}: {}", dest.display(), e)),
    }
}

fn prepare_destination(
    dest: &Path,
    opts: &Options,
    desired_target: &Path,
) -> io::Result<Preparation> {
    let meta = match fs::symlink_metadata(dest) {
        Ok(meta) => meta,
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                return Ok(Preparation::Create);
            }

            return Err(e);
        }
    };

    if meta.file_type().is_symlink() {
        if opts.symbolic && !opts.force && !opts.backup && !opts.interactive {
            if let Ok(current) = fs::read_link(dest) {
                if current == desired_target {
                    return Ok(Preparation::Skip);
                }
            }
        }

        if opts.interactive && !prompt_yes(&format!("ln: replace '{}'? ", dest.display())) {
            return Ok(Preparation::Skip);
        }

        if opts.backup {
            backup_destination(dest)?;
            return Ok(Preparation::Create);
        }

        if opts.force || opts.symbolic {
            fs::remove_file(dest)?;
            return Ok(Preparation::Create);
        }

        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "File exists",
        ));
    }

    if meta.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "cannot overwrite directory",
        ));
    }

    if opts.interactive && !prompt_yes(&format!("ln: replace '{}'? ", dest.display())) {
        return Ok(Preparation::Skip);
    }

    if opts.backup {
        backup_destination(dest)?;
        return Ok(Preparation::Create);
    }

    if opts.force {
        fs::remove_file(dest)?;
        return Ok(Preparation::Create);
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "File exists",
    ))
}

fn backup_destination(path: &Path) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid path"))?;

    let mut backup = path.to_path_buf();
    backup.set_file_name(format!("{}~", name.to_string_lossy()));

    fs::rename(path, backup)
}

fn prompt_yes(question: &str) -> bool {
    eprint!("{}", question);
    let _ = io::stderr().flush();

    let mut line = String::new();

    if io::stdin().read_line(&mut line).is_err() {
        return false;
    }

    matches!(
        line.trim_start(),
        "y" | "Y" | "yes" | "Yes" | "YES"
    )
}

fn hard_link_with_mode(source: &Path, dest: &Path, logical: bool) -> io::Result<()> {
    let src = c_string(source)?;
    let dst = c_string(dest)?;

    let flags = if logical {
        libc::AT_SYMLINK_FOLLOW
    } else {
        0
    };

    let res = unsafe {
        libc::linkat(
            libc::AT_FDCWD,
            src.as_ptr(),
            libc::AT_FDCWD,
            dst.as_ptr(),
            flags,
        )
    };

    if res == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn c_string(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL byte")
    })
}

fn make_symlink_target_text(
    source: &Path,
    dest: &Path,
    relative: bool,
) -> io::Result<PathBuf> {
    if !relative {
        return Ok(source.to_path_buf());
    }

    let cwd = env::current_dir()?;

    let source_abs = if source.is_absolute() {
        normalize_path(source)
    } else {
        normalize_path(&cwd.join(source))
    };

    let dest_parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    let dest_dir_abs = if dest_parent.is_absolute() {
        normalize_path(dest_parent)
    } else {
        normalize_path(&cwd.join(dest_parent))
    };

    Ok(relative_path(&dest_dir_abs, &source_abs))
}

fn normalize_path(path: &Path) -> PathBuf {
    let absolute = path.is_absolute();
    let mut result = PathBuf::new();

    for comp in path.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if absolute {
                    if result.parent().is_some() {
                        result.pop();
                    }
                } else if result.file_name().is_some() {
                    result.pop();
                } else {
                    result.push("..");
                }
            }
            other => {
                result.push(other.as_os_str());
            }
        }
    }

    if result.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        result
    }
}

fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let mut from_comps: Vec<_> = from.components().collect();
    let mut to_comps: Vec<_> = to.components().collect();

    while !from_comps.is_empty()
        && !to_comps.is_empty()
        && from_comps[0] == to_comps[0]
    {
        from_comps.remove(0);
        to_comps.remove(0);
    }

    let mut out = PathBuf::new();

    for _ in &from_comps {
        out.push("..");
    }

    for comp in &to_comps {
        out.push(comp.as_os_str());
    }

    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

fn file_name_of(path: &Path) -> PathBuf {
    match path.file_name() {
        Some(name) => PathBuf::from(name),
        None => PathBuf::from(path.as_os_str()),
    }
}

fn is_directory_follow(path: &Path) -> bool {
    path.is_dir()
}

fn is_existing_directory(path: &Path, follow_symlink_dirs: bool) -> bool {
    if follow_symlink_dirs {
        path.is_dir()
    } else {
        match fs::symlink_metadata(path) {
            Ok(meta) => meta.is_dir() && !meta.file_type().is_symlink(),
            Err(_) => false,
        }
    }
}

fn is_symlink_path(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) => meta.file_type().is_symlink(),
        Err(_) => false,
    }
}

fn has_trailing_slash(path: &Path) -> bool {
    path.to_string_lossy().ends_with('/')
}
