// Очищенный вариант expr.rs для копирования
use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("expr: missing operand");
        std::process::exit(2);
    }

    if args[0] == "length" && args.len() == 2 {
        println!("{}", args[1].chars().count());
        std::process::exit(0);
    }

    if args.len() == 3 {
        let left = &args[0];
        let op = &args[1];
        let right = &args[2];

        let left_num = left.parse::<i64>();
        let right_num = right.parse::<i64>();

        match (left_num, right_num) {
            (Ok(l), Ok(r)) => {
                let result = match op.as_str() {
                    "+" => Some(l.wrapping_add(r)),
                    "-" => Some(l.wrapping_sub(r)),
                    "*" => Some(l.wrapping_mul(r)),
                    "/" => {
                        if r == 0 {
                            eprintln!("expr: division by zero");
                            std::process::exit(3);
                        }
                        Some(l / r)
                    }
                    "%" => {
                        if r == 0 {
                            eprintln!("expr: division by zero");
                            std::process::exit(3);
                        }
                        Some(l % r)
                    }
                    "=" | "==" => Some(if l == r { 1 } else { 0 }),
                    "!=" => Some(if l != r { 1 } else { 0 }),
                    "<" => Some(if l < r { 1 } else { 0 }),
                    "<=" => Some(if l <= r { 1 } else { 0 }),
                    ">" => Some(if l > r { 1 } else { 0 }),
                    ">=" => Some(if l >= r { 1 } else { 0 }),
                    _ => None,
                };

                if let Some(val) = result {
                    println!("{}", val);
                    std::process::exit(if val == 0 { 1 } else { 0 });
                }
            }
            _ => {
                let val = match op.as_str() {
                    "=" | "==" => {
                        if left == right {
                            1
                        } else {
                            0
                        }
                    }
                    "!=" => {
                        if left != right {
                            1
                        } else {
                            0
                        }
                    }
                    _ => {
                        eprintln!("expr: non-integer argument");
                        std::process::exit(2);
                    }
                };
                println!("{}", val);
                std::process::exit(if val == 0 { 1 } else { 0 });
            }
        }
    }

    println!("{}", args[0]);
    std::process::exit(0);
}
