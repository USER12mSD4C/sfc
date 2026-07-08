use std::env;
use std::io::{self, Write};

fn main() {
    let args: Vec<_> = env::args_os().collect();
    let string_to_print = if args.len() > 1 {
        let mut s = args[1].to_string_lossy().into_owned();
        s.push('\n');
        s
    } else {
        "y\n".to_string()
    };

    let bytes = string_to_print.as_bytes();

    // Создаем большой буфер на 64 КБ
    let mut buffer = Vec::with_capacity(65536);
    while buffer.len() + bytes.len() <= 65536 {
        buffer.extend_from_slice(bytes);
    }
    if buffer.is_empty() {
        buffer.extend_from_slice(bytes);
    }

    let mut stdout = io::stdout().lock();
    loop {
        // Если труба (pipe) закрыта на чтение, мирно выходим из процесса
        if stdout.write_all(&buffer).is_err() {
            break;
        }
    }
}
