use std::env;
use std::fs::File;
use std::io::{self, Read};

fn crc32_posix(data: &[u8]) -> u32 {
    const POLY: u32 = 0x04C11DB7;
    let mut crc: u32 = 0;
    for &b in data {
        for i in 0..8 {
            let bit = ((b >> (7 - i)) & 1) as u32;
            let c = (crc >> 31) & 1;
            crc <<= 1;
            if c ^ bit != 0 {
                crc ^= POLY;
            }
        }
    }
    let mut len = data.len();
    while len > 0 {
        let b = (len & 0xff) as u8;
        for i in 0..8 {
            let bit = ((b >> (7 - i)) & 1) as u32;
            let c = (crc >> 31) & 1;
            crc <<= 1;
            if c ^ bit != 0 {
                crc ^= POLY;
            }
        }
        len >>= 8;
    }
    !crc
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        let mut buffer = Vec::new();
        if io::stdin().read_to_end(&mut buffer).is_ok() {
            let crc = crc32_posix(&buffer);
            println!("{} {}", crc, buffer.len());
        }
        return;
    }

    for arg in args {
        match File::open(&arg) {
            Ok(mut file) => {
                let mut buffer = Vec::new();
                if file.read_to_end(&mut buffer).is_ok() {
                    let crc = crc32_posix(&buffer);
                    println!("{} {} {}", crc, buffer.len(), arg);
                }
            }
            Err(e) => {
                eprintln!("cksum: {}: {}", arg, e);
            }
        }
    }
}
