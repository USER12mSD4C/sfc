use std::env;

fn to_iec(mut val: f64) -> String {
    let units = ["", "K", "M", "G", "T", "P", "E"];
    let mut idx = 0;
    while val >= 1024.0 && idx < units.len() - 1 {
        val /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{:.0}", val)
    } else {
        format!("{:.1}{}", val, units[idx])
    }
}

fn main() {
    let args: Vec<_> = env::args_os().collect();
    let mut to_iec_mode = false;
    let mut numbers = Vec::new();

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "--to=iec" {
            to_iec_mode = true;
        } else if s.starts_with('-') {
            eprintln!("numfmt: invalid option: {}", s);
            std::process::exit(1);
        } else {
            numbers.push(s.into_owned());
        }
    }

    if numbers.is_empty() {
        eprintln!("Usage: numfmt [--to=iec] <number1> ...");
        std::process::exit(1);
    }

    for num_str in numbers {
        if let Ok(val) = num_str.parse::<f64>() {
            if to_iec_mode {
                println!("{}", to_iec(val));
            } else {
                println!("{}", val);
            }
        } else {
            eprintln!("numfmt: invalid number: {}", num_str);
            std::process::exit(1);
        }
    }
}
