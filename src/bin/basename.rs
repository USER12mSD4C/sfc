use std::env;
use std::path::Path;

fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        std::process::exit(1);
    }
    let path = Path::new(&args[1]);
    println!(
        "{}",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    );
}
