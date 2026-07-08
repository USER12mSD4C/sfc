use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut create_dirs = false;
    let mut mode_str = None;
    let mut targets = Vec::new();

    let mut i = 1;
    while i < args.len() {
        let s = args[i].to_string_lossy();
        if s == "-d" {
            create_dirs = true;
            i += 1;
        } else if s == "-m" && i + 1 < args.len() {
            mode_str = Some(args[i + 1].to_string_lossy().into_owned());
            i += 2;
        } else if s.starts_with('-') {
            eprintln!("install: invalid option: {}", s);
            std::process::exit(1);
        } else {
            targets.push(Path::new(&args[i]));
            i += 1;
        }
    }

    let mode = if let Some(ref m) = mode_str {
        u32::from_str_radix(m, 8).unwrap_or(0o755)
    } else {
        0o755
    };

    if create_dirs {
        for path in targets {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true).mode(mode);
            builder.create(path)?;
        }
        return Ok(());
    }

    if targets.len() < 2 {
        eprintln!("Usage: install [-m mode] <source> <target>");
        std::process::exit(1);
    }

    let src = targets[0];
    let mut dest = targets[1].to_path_buf();

    if dest.is_dir() {
        if let Some(file_name) = src.file_name() {
            dest.push(file_name);
        }
    }

    fs::copy(src, &dest)?;
    fs::set_permissions(&dest, fs::Permissions::from_mode(mode))?;

    Ok(())
}
