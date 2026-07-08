use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn format_time(t: SystemTime) -> String {
    if let Ok(duration) = t.duration_since(UNIX_EPOCH) {
        let secs = duration.as_secs();
        let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
        unsafe {
            libc::localtime_r(&(secs as libc::time_t), &mut tm);
        }
        let mut buf = [0u8; 64];
        let len = unsafe {
            libc::strftime(
                buf.as_mut_ptr() as *mut libc::c_char,
                buf.len(),
                b"%Y-%m-%d %H:%M:%S\0".as_ptr() as *const libc::c_char,
                &tm,
            )
        };
        if len > 0 {
            return String::from_utf8_lossy(&buf[..len]).into_owned();
        }
    }
    "-".to_string()
}

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        eprintln!("Usage: stat <file1> [file2 ...]");
        std::process::exit(1);
    }

    for file in args.iter().skip(1) {
        let path = Path::new(file);
        let meta = fs::symlink_metadata(path)?;

        println!("  File: {}", path.to_string_lossy());
        println!(
            "  Size: {:<15} Blocks: {:<10} IO Block: {:<10} {}",
            meta.len(),
            meta.blocks(),
            meta.blksize(),
            if meta.is_dir() {
                "directory"
            } else if meta.is_symlink() {
                "symbolic link"
            } else {
                "regular file"
            }
        );
        println!(
            "Device: {:<15} Inode: {:<10} Links: {:<10}",
            meta.dev(),
            meta.ino(),
            meta.nlink()
        );
        println!(
            "Access: ({:04o})  Uid: ({:<5})   Gid: ({:<5})",
            meta.mode() & 0o7777,
            meta.uid(),
            meta.gid()
        );

        if let Ok(atime) = meta.accessed() {
            println!("Access: {}", format_time(atime));
        }
        if let Ok(mtime) = meta.modified() {
            println!("Modify: {}", format_time(mtime));
        }
    }
    Ok(())
}
