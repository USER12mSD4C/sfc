use std::env;
use std::thread;
use std::time::Duration;

fn main() {
    let args: Vec<_> = env::args().collect();
    if args.len() < 2 {
        std::process::exit(1);
    }
    if let Ok(secs) = args[1].parse::<u64>() {
        thread::sleep(Duration::from_secs(secs));
    } else {
        std::process::exit(1);
    }
}
