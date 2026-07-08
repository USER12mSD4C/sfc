use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        eprintln!("Usage: shred <file1> [file2 ...]");
        std::process::exit(1);
    }

    let mut urandom = std::fs::File::open("/dev/urandom")?;
    let mut rand_buf = [0u8; 65536];

    for file in args.iter().skip(1) {
        let path = Path::new(file);
        let metadata = fs::metadata(path)?;
        let size = metadata.len();

        let mut f = OpenOptions::new().write(true).open(path)?;

        // Шаг 1: Перезапись случайными байтами
        let mut bytes_written = 0;
        while bytes_written < size {
            let to_write = std::cmp::min(rand_buf.len() as u64, size - bytes_written) as usize;
            urandom.read_exact(&mut rand_buf[..to_write])?;
            f.write_all(&rand_buf[..to_write])?;
            bytes_written += to_write as u64;
        }
        f.sync_all()?;

        // Шаг 2: Забивание нулями
        f.seek(SeekFrom::Start(0))?;
        let zero_buf = [0u8; 65536];
        bytes_written = 0;
        while bytes_written < size {
            let to_write = std::cmp::min(zero_buf.len() as u64, size - bytes_written) as usize;
            f.write_all(&zero_buf[..to_write])?;
            bytes_written += to_write as u64;
        }
        f.sync_all()?;

        // Шаг 3: Удаление файла
        drop(f);
        fs::remove_file(path)?;
    }
    Ok(())
}
