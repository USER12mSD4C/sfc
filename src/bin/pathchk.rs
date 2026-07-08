use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: pathchk <path> ...");
        std::process::exit(1);
    }

    let mut exit_code = 0;
    for arg in args {
        let path = Path::new(&arg);
        if arg.len() > 4096 {
            eprintln!("pathchk: '{}': Path too long", arg);
            exit_code = 1;
            continue;
        }
        for comp in path.components() {
            if let Some(s) = comp.as_os_str().to_str() {
                if s.len() > 255 {
                    eprintln!("pathchk: '{}': Name component too long", s);
                    exit_code = 1;
                }
            }
        }
    }
    std::process::exit(exit_code);
}
