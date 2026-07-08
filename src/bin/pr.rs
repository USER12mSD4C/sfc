use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut file_path = None;
    if args.len() > 1 {
        file_path = Some(Path::new(&args[1]));
    }

    let reader: Box<dyn BufRead> = if let Some(p) = file_path {
        Box::new(BufReader::new(File::open(p)?))
    } else {
        Box::new(BufReader::new(io::stdin()))
    };

    let file_name = file_path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "".to_string());

    let mut page = 1;
    let mut line_count = 0;
    let page_length = 56; // Стандартная длина печатной страницы в pr

    println!("\n\n2026-07-08 19:04  {}  Page {}\n\n", file_name, page);

    for line in reader.lines() {
        let line = line?;
        println!("{}", line);
        line_count += 1;
        if line_count >= page_length {
            page += 1;
            line_count = 0;
            // Символ перевода страницы \x0c (Form Feed) и новая шапка
            println!("\x0c\n\n2026-07-08 19:04  {}  Page {}\n\n", file_name, page);
        }
    }
    Ok(())
}
