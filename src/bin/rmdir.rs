use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("rmdir: missing operand");
        std::process::exit(1);
    }

    let mut exit_code = 0;
    for arg in args {
        if let Err(e) = fs::remove_dir(&arg) {
            eprintln!("rmdir: failed to remove '{}': {}", arg, e);
            exit_code = 1;
        }
    }
    std::process::exit(exit_code);
}
