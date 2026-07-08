use std::env;

fn posix_dirname(s: &str) -> String {
    if s.is_empty() {
        return ".".to_string();
    }

    // Удаляем завершающие слеши (кроме случая, когда вся строка состоит из слешей)
    let mut trimmed = s;
    while trimmed.len() > 1 && trimmed.ends_with('/') {
        trimmed = &trimmed[..trimmed.len() - 1];
    }

    if trimmed == "/" {
        return "/".to_string();
    }

    // Ищем последний слеш
    if let Some(idx) = trimmed.rfind('/') {
        if idx == 0 {
            return "/".to_string();
        }
        let mut dir = &trimmed[..idx];
        // Удаляем завершающие слеши у получившегося пути
        while dir.len() > 1 && dir.ends_with('/') {
            dir = &dir[..dir.len() - 1];
        }
        dir.to_string()
    } else {
        ".".to_string()
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("dirname: missing operand");
        std::process::exit(1);
    }

    for arg in &args[1..] {
        println!("{}", posix_dirname(arg));
    }
}
