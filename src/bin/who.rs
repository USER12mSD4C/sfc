use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    unsafe {
        // Инициализируем указатель чтения системной базы данных сессий
        libc::setutxent();

        loop {
            let ut = libc::getutxent();
            if ut.is_null() {
                break;
            }

            // Фильтруем только реальные активные сессии пользователей
            if (*ut).ut_type as libc::c_int == libc::USER_PROCESS as libc::c_int {
                // Извлекаем имя пользователя (ut_user)
                let user_bytes = &(*ut).ut_user;
                let mut u_len = 0;
                while u_len < user_bytes.len() && user_bytes[u_len] != 0 {
                    u_len += 1;
                }
                let u8_user: Vec<u8> = user_bytes[..u_len].iter().map(|&c| c as u8).collect();
                let username = String::from_utf8_lossy(&u8_user).into_owned();

                // Извлекаем имя tty-линии (ut_line)
                let line_bytes = &(*ut).ut_line;
                let mut l_len = 0;
                while l_len < line_bytes.len() && line_bytes[l_len] != 0 {
                    l_len += 1;
                }
                let u8_line: Vec<u8> = line_bytes[..l_len].iter().map(|&c| c as u8).collect();
                let line = String::from_utf8_lossy(&u8_line).into_owned();

                // Извлекаем имя удаленного хоста или дисплея (ut_host)
                let host_bytes = &(*ut).ut_host;
                let mut h_len = 0;
                while h_len < host_bytes.len() && host_bytes[h_len] != 0 {
                    h_len += 1;
                }
                let u8_host: Vec<u8> = host_bytes[..h_len].iter().map(|&c| c as u8).collect();
                let host = String::from_utf8_lossy(&u8_host).into_owned();

                // ИСПРАВЛЕНИЕ: кастуем к системному типу time_t перед передачей указателя
                let tv_sec: libc::time_t = (*ut).ut_tv.tv_sec as libc::time_t;
                let mut tm = std::mem::zeroed::<libc::tm>();
                libc::localtime_r(&tv_sec, &mut tm);
                let mut time_buf = [0u8; 64];
                let t_len = libc::strftime(
                    time_buf.as_mut_ptr() as *mut libc::c_char,
                    time_buf.len(),
                    b"%Y-%m-%d %H:%M\0".as_ptr() as *const libc::c_char,
                    &tm,
                );
                let time_str = String::from_utf8_lossy(&time_buf[..t_len]).into_owned();

                if !username.is_empty() {
                    if host.is_empty() {
                        writeln!(stdout, "{:<12} {:<12} {}", username, line, time_str)?;
                    } else {
                        writeln!(
                            stdout,
                            "{:<12} {:<12} {:<16} ({})",
                            username, line, time_str, host
                        )?;
                    }
                }
            }
        }
        libc::endutxent();
    }
    Ok(())
}
