use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();

    // Если бинарник запущен как "[", последний аргумент обязан быть "]"
    if let Some(arg0) = env::args().next() {
        if Path::new(&arg0).file_name().and_then(|s| s.to_str()) == Some("[") {
            if args.last().map(|s| s.as_str()) == Some("]") {
                args.pop();
            } else {
                std::process::exit(2); // Ошибка синтаксиса
            }
        }
    }

    if args.is_empty() {
        std::process::exit(1); // false
    }

    let result = evaluate(&args);
    if result {
        std::process::exit(0); // true
    } else {
        std::process::exit(1); // false
    }
}

fn evaluate(args: &[String]) -> bool {
    if args.len() == 1 {
        return !args[0].is_empty();
    }

    if args.len() == 2 {
        let op = &args[0];
        let val = &args[1];
        return match op.as_str() {
            "-z" => val.is_empty(),
            "-n" => !val.is_empty(),
            "-e" => Path::new(val).exists(),
            "-f" => fs::metadata(val).map(|m| m.is_file()).unwrap_or(false),
            "-d" => fs::metadata(val).map(|m| m.is_dir()).unwrap_or(false),
            _ => false,
        };
    }

    if args.len() == 3 {
        let left = &args[0];
        let op = &args[1];
        let right = &args[2];
        return match op.as_str() {
            "=" => left == right,
            "!=" => left != right,
            "-eq" => left.parse::<i64>().unwrap_or(0) == right.parse::<i64>().unwrap_or(0),
            "-ne" => left.parse::<i64>().unwrap_or(0) != right.parse::<i64>().unwrap_or(0),
            _ => false,
        };
    }

    false
}
