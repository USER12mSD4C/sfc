use std::env;
use std::fs;
use std::io;
use std::path::Path;

fn get_dir_size(path: &Path) -> io::Result<u64> {
    let mut total = 0;
    let metadata = fs::symlink_metadata(path)?;
    total += metadata.len();

    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            total += get_dir_size(&entry.path()).unwrap_or(0);
        }
    }
    Ok(total)
}

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let target = if args.len() > 1 {
        Path::new(&args[1])
    } else {
        Path::new(".")
    };

    if target.is_dir() {
        for entry in fs::read_dir(target)? {
            let entry = entry?;
            let path = entry.path();
            let size = get_dir_size(&path)?;
            println!("{}\t{}", size / 1024, path.to_string_lossy());
        }
    } else {
        let size = fs::metadata(target)?.len();
        println!("{}\t{}", size / 1024, target.to_string_lossy());
    }
    Ok(())
}
