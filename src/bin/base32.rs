use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

fn encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i];
        let b1 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] } else { 0 };
        let b3 = if i + 3 < data.len() { data[i + 3] } else { 0 };
        let b4 = if i + 4 < data.len() { data[i + 4] } else { 0 };

        let c0 = b0 >> 3;
        let c1 = ((b0 & 7) << 2) | (b1 >> 6);
        let c2 = (b1 >> 1) & 31;
        let c3 = ((b1 & 1) << 4) | (b2 >> 4);
        let c4 = ((b2 & 15) << 1) | (b3 >> 7);
        let c5 = (b3 >> 2) & 31;
        let c6 = ((b3 & 3) << 3) | (b4 >> 5);
        let c7 = b4 & 31;

        out.push(CHARS[c0 as usize]);
        out.push(CHARS[c1 as usize]);
        if i + 1 < data.len() {
            out.push(CHARS[c2 as usize]);
        } else {
            out.push(b'=');
        }
        if i + 1 < data.len() {
            out.push(CHARS[c3 as usize]);
        } else {
            out.push(b'=');
        }
        if i + 2 < data.len() {
            out.push(CHARS[c4 as usize]);
        } else {
            out.push(b'=');
        }
        if i + 3 < data.len() {
            out.push(CHARS[c5 as usize]);
        } else {
            out.push(b'=');
        }
        if i + 3 < data.len() {
            out.push(CHARS[c6 as usize]);
        } else {
            out.push(b'=');
        }
        if i + 4 < data.len() {
            out.push(CHARS[c7 as usize]);
        } else {
            out.push(b'=');
        }

        i += 5;
    }
    out
}

fn decode(data: &[u8]) -> Option<Vec<u8>> {
    let mut map = [0xffu8; 256];
    for (idx, &b) in CHARS.iter().enumerate() {
        map[b as usize] = idx as u8;
    }

    let mut clean = Vec::new();
    for &b in data {
        if b.is_ascii_whitespace() {
            continue;
        }
        if b == b'=' {
            break;
        }
        if map[b as usize] == 0xff {
            return None;
        }
        clean.push(b);
    }

    let mut out = Vec::new();
    let mut i = 0;
    while i < clean.len() {
        let c0 = map[clean[i] as usize];
        let c1 = if i + 1 < clean.len() {
            map[clean[i + 1] as usize]
        } else {
            0
        };
        let c2 = if i + 2 < clean.len() {
            map[clean[i + 2] as usize]
        } else {
            0
        };
        let c3 = if i + 3 < clean.len() {
            map[clean[i + 3] as usize]
        } else {
            0
        };
        let c4 = if i + 4 < clean.len() {
            map[clean[i + 4] as usize]
        } else {
            0
        };
        let c5 = if i + 5 < clean.len() {
            map[clean[i + 5] as usize]
        } else {
            0
        };
        let c6 = if i + 6 < clean.len() {
            map[clean[i + 6] as usize]
        } else {
            0
        };
        let c7 = if i + 7 < clean.len() {
            map[clean[i + 7] as usize]
        } else {
            0
        };

        let b0 = (c0 << 3) | (c1 >> 2);
        let b1 = ((c1 & 3) << 6) | (c2 << 1) | (c3 >> 4);
        let b2 = ((c3 & 15) << 4) | (c4 >> 1);
        let b3 = ((c4 & 1) << 7) | (c5 << 2) | (c6 >> 3);
        let b4 = ((c6 & 7) << 5) | c7;

        out.push(b0);
        if i + 2 < clean.len() {
            out.push(b1);
        }
        if i + 4 < clean.len() {
            out.push(b2);
        }
        if i + 5 < clean.len() {
            out.push(b3);
        }
        if i + 7 < clean.len() {
            out.push(b4);
        }
        i += 8;
    }
    Some(out)
}

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut decode_mode = false;
    let mut file_path = None;

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "-d" || s == "--decode" {
            decode_mode = true;
        } else if s.starts_with('-') {
            eprintln!("base32: invalid option: {}", s);
            std::process::exit(1);
        } else {
            file_path = Some(s.into_owned());
        }
    }

    let mut input: Box<dyn Read> = if let Some(ref path) = file_path {
        Box::new(File::open(Path::new(path))?)
    } else {
        Box::new(io::stdin())
    };

    let mut buffer = Vec::new();
    input.read_to_end(&mut buffer)?;

    let mut stdout = io::stdout().lock();
    if decode_mode {
        if let Some(decoded) = decode(&buffer) {
            stdout.write_all(&decoded)?;
        } else {
            eprintln!("base32: invalid input");
            std::process::exit(1);
        }
    } else {
        let encoded = encode(&buffer);
        let mut idx = 0;
        while idx < encoded.len() {
            let end = std::cmp::min(idx + 76, encoded.len());
            stdout.write_all(&encoded[idx..end])?;
            stdout.write_all(b"\n")?;
            idx += 76;
        }
    }
    Ok(())
}
