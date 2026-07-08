use std::env;
use std::fs::File;
use std::io::{self, BufReader, Read, Write};

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: hex <file>");
        std::process::exit(1);
    }

    let file = File::open(&args[0])?;
    let mut reader = BufReader::new(file);
    let mut buffer = [0u8; 16];
    let mut offset = 0;

    let mut stdout = io::stdout().lock();

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        // Выводим офсет (фиолетовым цветом)
        write!(stdout, "\x1b[38;2;203;166;247m{:08x}\x1b[0m  ", offset)?;

        // Печатаем hex-байты
        for i in 0..16 {
            if i < bytes_read {
                let b = buffer[i];
                if b == 0 {
                    // Нули делаем тускло-серыми, чтобы они не мешали
                    write!(stdout, "\x1b[38;2;80;80;80m00 \x1b[0m")?;
                } else if b.is_ascii_alphanumeric() || b.is_ascii_graphic() {
                    // Печатные символы - приятным зеленым
                    write!(stdout, "\x1b[38;2;166;227;161m{:02x} \x1b[0m", b)?;
                } else {
                    // Остальное - пастельно-розовым
                    write!(stdout, "\x1b[38;2;243;139;168m{:02x} \x1b[0m", b)?;
                }
            } else {
                write!(stdout, "   ")?;
            }
            if i == 7 {
                write!(stdout, " ")?;
            }
        }

        write!(stdout, " |")?;

        // Печатаем ASCII представление справа
        for i in 0..bytes_read {
            let b = buffer[i];
            if b.is_ascii_graphic() || b == b' ' {
                write!(stdout, "\x1b[38;2;166;227;161m{}\x1b[0m", b as char)?;
            } else if b == 0 {
                write!(stdout, "\x1b[38;2;80;80;80m.\x1b[0m")?;
            } else {
                write!(stdout, "\x1b[38;2;243;139;168m.\x1b[0m")?;
            }
        }

        writeln!(stdout, "|")?;
        offset += bytes_read;
    }

    Ok(())
}
