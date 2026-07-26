use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process;

fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 3 {
        eprintln!("Usage: chmod <mode> <file1> [file2 ...]");
        process::exit(1);
    }

    let mode_arg = &args[1];
    let files = &args[2..];
    let mut exit_code = 0;

    let mode_is_plus_x = mode_arg == "+x";
    let mode_is_minus_x = mode_arg == "-x";

    let octal_mode = if !mode_is_plus_x && !mode_is_minus_x {
        let mode_str = match mode_arg.to_str() {
            Some(s) => s,
            None => {
                eprintln!("chmod: invalid mode: argument is not valid UTF-8");
                process::exit(1);
            }
        };
        match u32::from_str_radix(mode_str, 8) {
            Ok(v) => {
                if v > 0o7777 {
                    eprintln!("chmod: invalid mode: '{}'", mode_str);
                    process::exit(1);
                }
                Some(v)
            }
            Err(_) => {
                eprintln!("chmod: invalid mode: '{}'", mode_str);
                process::exit(1);
            }
        }
    } else {
        None
    };

    for file in files {
        let path = Path::new(file);

        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("chmod: cannot access '{}': {}", path.display(), e);
                exit_code = 1;
                continue;
            }
        };

        let mut perms = metadata.permissions();
        let current_mode = perms.mode();

        let new_mode = if mode_is_plus_x {
            current_mode | 0o111
        } else if mode_is_minus_x {
            current_mode & !0o111
        } else {
            octal_mode.unwrap()
        };

        perms.set_mode(new_mode);
        if let Err(e) = fs::set_permissions(path, perms) {
            eprintln!("chmod: changing permissions of '{}': {}", path.display(), e);
            exit_code = 1;
        }
    }

    if exit_code != 0 {
        process::exit(exit_code);
    }
}
