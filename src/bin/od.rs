use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let path = if args.len() > 1 {
        Some(Path::new(&args[1]))
    } else {
        None
    };

    let mut reader: Box<dyn Read> = if let Some(p) = path {
        Box::new(File::open(p)?)
    } else {
        Box::new(io::stdin())
    };

    let mut buffer = [0u8; 16];
    let mut address = 0;

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }

        print!("{:07o}", address);

        let mut i = 0;
        while i < n {
            let b0 = buffer[i];
            let b1 = if i + 1 < n { buffer[i + 1] } else { 0 };
            // Объединяем два байта в 16-битное слово native-endian (как в GNU od)
            let word = u16::from_ne_bytes([b0, b1]);
            print!(" {:06o}", word);
            i += 2;
        }
        println!();
        address += n;
    }

    if address > 0 {
        println!("{:07o}", address);
    }
    Ok(())
}
