// src/bin/rm.rs
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

const VERSION: &str = "rm (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: rm [OPTION]... [FILE]...\n\
Remove (unlink) the FILE(s).\n\
\n\
  -f, --force                  ignore nonexistent files, never prompt\n\
  -i, --interactive            prompt before every removal\n\
  -I, --interactive=once       prompt once before removing more than three files\n\
  -r, -R, --recursive          remove directories and their contents recursively\n\
  -d, --dir                    remove empty directories\n\
  -v, --verbose                explain what is being done\n\
      --preserve-root          do not remove '/' (default)\n\
      --no-preserve-root       do not treat '/' specially\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

struct Options {
    force: bool,
    interactive: bool,
    interactive_once: bool,
    recursive: bool,
    dir: bool,
    verbose: bool,
    preserve_root: bool,
    paths: Vec<PathBuf>,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        force: false,
        interactive: false,
        interactive_once: false,
        recursive: false,
        dir: false,
        verbose: false,
        preserve_root: true,
        paths: Vec::new(),
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
            if arg == "--preserve-root" {
                opts.preserve_root = true;
                continue;
            }
            if arg == "--no-preserve-root" {
                opts.preserve_root = false;
                continue;
            }

            for c in arg.chars().skip(1) {
                match c {
                    'f' => {
                        opts.force = true;
                        opts.interactive = false;
                    }
                    'i' => {
                        opts.interactive = true;
                        opts.force = false;
                    }
                    'I' => {
                        opts.interactive_once = true;
                        opts.force = false;
                    }
                    'r' | 'R' => opts.recursive = true,
                    'd' => opts.dir = true,
                    'v' => opts.verbose = true,
                    _ => return Err(format!("invalid option -- '{}'", c)),
                }
            }
        } else {
            opts.paths.push(PathBuf::from(arg));
        }
    }

    if opts.paths.is_empty() {
        if opts.force {
            return Ok(opts);
        }
        return Err("missing operand".into());
    }
    Ok(opts)
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("rm: {}", e);
            eprintln!("Try 'rm --help' for more information.");
            return ExitCode::from(1);
        }
    };

    if opts.interactive_once && opts.paths.len() > 3 {
        eprint!("rm: remove {} arguments? ", opts.paths.len());
        let mut ans = String::new();
        io::stdin().read_line(&mut ans).unwrap_or(0);
        if !ans.starts_with('y') && !ans.starts_with('Y') {
            return ExitCode::from(0);
        }
    }

    let mut had_error = false;

    for path in &opts.paths {
        let path_str = path.to_string_lossy();
        if opts.preserve_root && path_str == "/" {
            eprintln!("rm: it is dangerous to operate recursively on '/'");
            eprintln!("rm: use --no-preserve-root to override");
            had_error = true;
            continue;
        }

        let meta = fs::symlink_metadata(path);
        match meta {
            Ok(m) => {
                if opts.interactive && !opts.force {
                    eprint!(
                        "rm: remove {} '{}'? ",
                        if m.is_dir() { "directory" } else { "file" },
                        path.display()
                    );
                    let mut ans = String::new();
                    io::stdin().read_line(&mut ans).unwrap_or(0);
                    if !ans.starts_with('y') && !ans.starts_with('Y') {
                        continue;
                    }
                }

                let res = if m.is_dir() {
                    if opts.recursive {
                        fs::remove_dir_all(path)
                    } else if opts.dir {
                        fs::remove_dir(path)
                    } else {
                        eprintln!("rm: {}: Is a directory", path.display());
                        had_error = true;
                        continue;
                    }
                } else {
                    fs::remove_file(path)
                };

                match res {
                    Ok(_) => {
                        if opts.verbose {
                            println!("removed '{}'", path.display());
                        }
                    }
                    Err(e) => {
                        eprintln!("rm: {}: {}", path.display(), e);
                        had_error = true;
                    }
                }
            }
            Err(e) => {
                if !opts.force {
                    eprintln!("rm: {}: {}", path.display(), e);
                    had_error = true;
                }
            }
        }
    }

    if had_error {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}
