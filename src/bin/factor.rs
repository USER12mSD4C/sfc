use std::env;

fn factorize(mut n: u64) {
    print!("{}:", n);
    while n % 2 == 0 {
        print!(" 2");
        n /= 2;
    }
    let mut d = 3;
    while d * d <= n {
        while n % d == 0 {
            print!(" {}", d);
            n /= d;
        }
        d += 2;
    }
    if n > 1 {
        print!(" {}", n);
    }
    println!();
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        let mut line = String::new();
        while std::io::stdin().read_line(&mut line).unwrap_or(0) > 0 {
            for token in line.split_whitespace() {
                if let Ok(n) = token.parse::<u64>() {
                    factorize(n);
                } else {
                    eprintln!("factor: '{}' is not a valid positive integer", token);
                    std::process::exit(1);
                }
            }
            line.clear();
        }
    } else {
        for arg in args {
            if let Ok(n) = arg.parse::<u64>() {
                factorize(n);
            } else {
                eprintln!("factor: '{}' is not a valid positive integer", arg);
                std::process::exit(1);
            }
        }
    }
}
