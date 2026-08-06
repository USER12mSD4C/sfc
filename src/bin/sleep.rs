use std::env;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

fn main() -> ExitCode {
    let args: Vec<_> = env::args().collect();
    if args.len() < 2 {
        eprintln!("sleep: missing operand");
        return ExitCode::from(1);
    }

    match args[1].parse::<f64>() {
        Ok(secs) if secs >= 0.0 => {
            let nanos = (secs * 1_000_000_000.0) as u64;
            thread::sleep(Duration::from_nanos(nanos));
            ExitCode::from(0)
        }
        _ => {
            eprintln!("sleep: invalid time interval '{}'", args[1]);
            ExitCode::from(1)
        }
    }
}
