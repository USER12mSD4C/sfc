use std::env;
use std::io::{self, Read, Write};
use std::process::{Command, Stdio};

fn base64_encode(bytes: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i] as usize;
        let b1 = if i + 1 < bytes.len() {
            bytes[i + 1] as usize
        } else {
            0
        };
        let b2 = if i + 2 < bytes.len() {
            bytes[i + 2] as usize
        } else {
            0
        };

        let c0 = b0 >> 2;
        let c1 = ((b0 & 3) << 4) | (b1 >> 4);
        let c2 = ((b1 & 15) << 2) | (b2 >> 6);
        let c3 = b2 & 63;

        result.push(CHARSET[c0] as char);
        result.push(CHARSET[c1] as char);
        if i + 1 < bytes.len() {
            result.push(CHARSET[c2] as char);
        } else {
            result.push('=');
        }
        if i + 2 < bytes.len() {
            result.push(CHARSET[c3] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mode = args.get(0).map(|s| s.as_str()).unwrap_or("copy");

    if mode == "copy" {
        let mut buffer = Vec::new();
        io::stdin().read_to_end(&mut buffer)?;

        // 1. Попытка скопировать через Wayland (wl-clipboard)
        if env::var("WAYLAND_DISPLAY").is_ok() {
            if let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(&buffer)?;
                }
                let _ = child.wait();
                return Ok(());
            }
        }

        // 2. Попытка скопировать через X11 (xclip)
        if env::var("DISPLAY").is_ok() {
            if let Ok(mut child) = Command::new("xclip")
                .arg("-selection")
                .arg("clipboard")
                .stdin(Stdio::piped())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(&buffer)?;
                }
                let _ = child.wait();
                return Ok(());
            }
        }

        // 3. Универсальный fallback: терминальный протокол OSC 52
        // Работает везде (даже по SSH и в tmux) на любых современных терминалах (включая ваш Kitty)
        let b64 = base64_encode(&buffer);
        print!("\x1b]52;c;{}\x07", b64);
        let _ = io::stdout().flush();
    } else if mode == "paste" {
        // Чтение из буфера обмена (работает только локально)
        if env::var("WAYLAND_DISPLAY").is_ok() {
            if let Ok(output) = Command::new("wl-paste").output() {
                io::stdout().write_all(&output.stdout)?;
                return Ok(());
            }
        }
        if env::var("DISPLAY").is_ok() {
            if let Ok(output) = Command::new("xclip")
                .arg("-selection")
                .arg("clipboard")
                .arg("-o")
                .output()
            {
                io::stdout().write_all(&output.stdout)?;
                return Ok(());
            }
        }
        eprintln!("clip: paste is only supported inside local desktop environments");
    } else {
        eprintln!("Usage: clip [copy|paste]");
    }
    Ok(())
}
