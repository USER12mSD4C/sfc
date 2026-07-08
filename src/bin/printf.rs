use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("printf: missing operand");
        std::process::exit(1);
    }

    let format = &args[0];
    let format_args = &args[1..];
    let mut arg_idx = 0;

    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next_c) = chars.next() {
                match next_c {
                    'n' => print!("\n"),
                    't' => print!("\t"),
                    'r' => print!("\r"),
                    '\\' => print!("\\"),
                    '\"' => print!("\""),
                    'a' => print!("\x07"), // bell
                    'b' => print!("\x08"), // backspace
                    'f' => print!("\x0c"), // form feed
                    'v' => print!("\x0b"), // vertical tab
                    _ => print!("\\{}", next_c),
                }
            } else {
                print!("\\");
            }
        } else if c == '%' {
            if let Some(&next_c) = chars.peek() {
                if next_c == '%' {
                    chars.next();
                    print!("%");
                    continue;
                }

                let specifier = chars.next().unwrap();
                if arg_idx < format_args.len() {
                    let arg = &format_args[arg_idx];
                    arg_idx += 1;

                    match specifier {
                        's' => print!("{}", arg),
                        'd' | 'i' => {
                            let num = arg.parse::<i64>().unwrap_or(0);
                            print!("{}", num);
                        }
                        'u' => {
                            let num = arg.parse::<u64>().unwrap_or(0);
                            print!("{}", num);
                        }
                        'x' => {
                            let num = arg.parse::<i64>().unwrap_or(0);
                            print!("{:x}", num);
                        }
                        'X' => {
                            let num = arg.parse::<i64>().unwrap_or(0);
                            print!("{:X}", num);
                        }
                        'f' => {
                            let num = arg.parse::<f64>().unwrap_or(0.0);
                            // Си-стандарт требует ровно 6 знаков после запятой по умолчанию
                            print!("{:.6}", num);
                        }
                        _ => {
                            print!("{}", arg);
                        }
                    }
                } else {
                    match specifier {
                        's' => {}
                        'd' | 'i' | 'u' | 'x' | 'X' => print!("0"),
                        'f' => print!("0.000000"),
                        _ => {}
                    }
                }
            } else {
                print!("%");
            }
        } else {
            print!("{}", c);
        }
    }
}
