use std::env;

fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 || args.len() > 4 {
        eprintln!("Usage: seq [first [increment]] last");
        std::process::exit(1);
    }

    let mut first = 1.0;
    let mut incr = 1.0;
    let last;

    if args.len() == 2 {
        last = args[1].to_string_lossy().parse::<f64>().unwrap_or(0.0);
    } else if args.len() == 3 {
        first = args[1].to_string_lossy().parse::<f64>().unwrap_or(1.0);
        last = args[2].to_string_lossy().parse::<f64>().unwrap_or(0.0);
    } else {
        first = args[1].to_string_lossy().parse::<f64>().unwrap_or(1.0);
        incr = args[2].to_string_lossy().parse::<f64>().unwrap_or(1.0);
        last = args[3].to_string_lossy().parse::<f64>().unwrap_or(0.0);
    }

    if incr == 0.0 {
        eprintln!("seq: zero increment");
        std::process::exit(1);
    }

    let mut val = first;
    let mut stdout = std::io::stdout().lock();
    if incr > 0.0 {
        while val <= last {
            let _ = std::io::Write::write_all(&mut stdout, format!("{}\n", val).as_bytes());
            val += incr;
        }
    } else {
        while val >= last {
            let _ = std::io::Write::write_all(&mut stdout, format!("{}\n", val).as_bytes());
            val += incr;
        }
    }
}
