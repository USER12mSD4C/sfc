// src/bin/rmdir.rs
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::process::ExitCode;

const VERSION: &str = "rmdir (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: rmdir [OPTION]... DIRECTORY...\n\
Remove the DIRECTORY(ies), if they are empty.\n\
\n\
      --ignore-fail-on-non-empty   ignore each failure that is solely because a directory is non-empty\n\
  -p, --parents                    remove DIRECTORY and its ancestors; e.g., 'rmdir -p a/b/c' removes a/b/c, a/b, and a\n\
  -v, --verbose                    output a diagnostic for every directory processed\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

struct Options {
    ignore_fail: bool,
    parents: bool,
    verbose: bool,
    dirs: Vec<String>,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        ignore_fail: false,
        parents: false,
        verbose: false,
        dirs: Vec::new(),
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
            if arg == "--ignore-fail-on-non-empty" {
                opts.ignore_fail = true;
                continue;
            }

            for c in arg.chars().skip(1) {
                match c {
                    'p' => opts.parents = true,
                    'v' => opts.verbose = true,
                    _ => return Err(format!("invalid option -- '{}'", c)),
                }
            }
        } else {
            opts.dirs.push(arg);
        }
    }

    if opts.dirs.is_empty() {
        return Err("missing operand".into());
    }
    Ok(opts)
}

fn remove_dir_with_parents(path: &str, opts: &Options) -> io::Result<()> {
    let mut current = Path::new(path);
    loop {
        if opts.verbose {
            eprintln!("rmdir: removing directory, '{}'", current.display());
        }
        fs::remove_dir(current)?;
        if !opts.parents {
            break;
        }
        match current.parent() {
            Some(p) if p != Path::new("") && p != Path::new("/") => current = p,
            _ => break,
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("rmdir: {}", e);
            eprintln!("Try 'rmdir --help' for more information.");
            return ExitCode::from(1);
        }
    };

    let mut had_error = false;

    for dir in &opts.dirs {
        let res = remove_dir_with_parents(dir, &opts);
        if let Err(e) = res {
            let is_non_empty = e.kind() == io::ErrorKind::DirectoryNotEmpty;
            if !(opts.ignore_fail && is_non_empty) {
                eprintln!("rmdir: failed to remove '{}': {}", dir, e);
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
