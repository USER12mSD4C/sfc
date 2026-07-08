use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 3 {
        eprintln!("Usage: chmod <mode> <file1> [file2 ...]");
        std::process::exit(1);
    }

    let mode_os = &args[1];
    let mode_str = mode_os.to_string_lossy();
    let files = &args[2..];

    for file in files {
        let path = Path::new(file);
        let metadata = fs::metadata(path)?;
        let mut perms = metadata.permissions();
        let current_mode = perms.mode();

        let new_mode = if mode_str == "+x" {
            current_mode | 0o111
        } else if mode_str == "-x" {
            current_mode & !0o111
        } else if let Ok(octal) = u32::from_str_radix(&mode_str, 8) {
            octal
        } else {
            eprintln!("chmod: unsupported mode: {}", mode_str);
            std::process::exit(1);
        };

        perms.set_mode(new_mode);
        fs::set_permissions(path, perms)?;
    }

    Ok(())
}
