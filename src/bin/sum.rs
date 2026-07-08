use std::env;
use std::fs::File;
use std::io::{self, Read};

fn bsd_checksum(data: &[u8]) -> (u16, usize) {
    let mut checksum: u16 = 0;
    for &b in data {
        checksum = (checksum >> 1) + ((checksum & 1) << 15);
        checksum = checksum.wrapping_add(b as u16);
    }
    let blocks = (data.len() + 1023) / 1024;
    (checksum, blocks)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        let mut buffer = Vec::new();
        if io::stdin().read_to_end(&mut buffer).is_ok() {
            let (sum, blocks) = bsd_checksum(&buffer);
            println!("{:05} {:5}", sum, blocks);
        }
        return;
    }

    for arg in args {
        match File::open(&arg) {
            Ok(mut file) => {
                let mut buffer = Vec::new();
                if file.read_to_end(&mut buffer).is_ok() {
                    let (sum, blocks) = bsd_checksum(&buffer);
                    println!("{:05} {:5} {}", sum, blocks, arg);
                }
            }
            Err(e) => {
                eprintln!("sum: {}: {}", arg, e);
            }
        }
    }
}
