use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;

fn get_random_string(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    if let Ok(mut f) = File::open("/dev/urandom") {
        let _ = f.read_exact(&mut bytes);
    } else {
        for i in 0..len {
            bytes[i] = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                % 256) as u8;
        }
    }
    let charset = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    bytes
        .iter()
        .map(|&b| charset[(b as usize) % charset.len()] as char)
        .collect()
}

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    let mut create_dir = false;
    let mut template = None;

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') && s != "-" {
            for c in s.chars().skip(1) {
                match c {
                    'd' => create_dir = true,
                    _ => {
                        eprintln!("mktemp: invalid option: -{}", c);
                        std::process::exit(1);
                    }
                }
            }
        } else {
            template = Some(s.into_owned());
        }
    }

    let tmp_dir = env::var("TMPDIR")
        .or_else(|_| env::var("TEMP"))
        .or_else(|_| env::var("TMP"))
        .unwrap_or_else(|_| "/tmp".to_string());

    let template_str = template.unwrap_or_else(|| format!("{}/tmp.XXXXXXXXXX", tmp_dir));

    let mut final_path_str = template_str.clone();
    if let Some(x_pos) = final_path_str.find("XXXXXX") {
        let x_count = final_path_str[x_pos..]
            .chars()
            .take_while(|&c| c == 'X')
            .count();
        let rand_str = get_random_string(x_count);
        final_path_str.replace_range(x_pos..(x_pos + x_count), &rand_str);
    } else {
        let rand_str = get_random_string(10);
        final_path_str.push_str(&rand_str);
    }

    let path = Path::new(&final_path_str);

    if create_dir {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(path)?;
    } else {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
    }

    println!("{}", path.to_string_lossy());
    Ok(())
}
