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

    let reader: Box<dyn BufRead> = if let Some(path) = file_path {
        Box::new(BufReader::new(File::open(path)?))
    } else {
        Box::new(BufReader::new(io::stdin()))
    };

    let mut line_num = 1;
    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            println!();
        } else {
            println!("{:>6}\t{}", line_num, line);
            line_num += 1;
        }
    }
    Ok(())
}
