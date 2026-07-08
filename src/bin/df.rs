use std::env;
use std::io;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let path = if args.len() > 1 {
        args[1].to_string_lossy().into_owned()
    } else {
        "/".to_string()
    };

    let mut stats = unsafe { std::mem::zeroed::<libc::statvfs>() };
    let c_path = std::ffi::CString::new(path.as_bytes()).unwrap();

    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stats) } < 0 {
        return Err(io::Error::last_os_error());
    }

    let block_size = stats.f_frsize;
    let total = (stats.f_blocks as u64 * block_size) / 1024;
    let free = (stats.f_bfree as u64 * block_size) / 1024;
    let used = total - free;
    let use_pct = if total > 0 { (used * 100) / total } else { 0 };

    println!("Filesystem     1K-blocks      Used Available Use%");
    println!(
        "{:<14} {:>10} {:>9} {:>9} {}%",
        path, total, used, free, use_pct
    );
    Ok(())
}
