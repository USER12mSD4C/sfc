use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        eprintln!("basename: missing operand");
        return ExitCode::from(1);
    }
    let path = Path::new(&args[1]);
    println!(
        "{}",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    );
    ExitCode::from(0)
}
