use std::ffi::CStr;

fn main() {
    println!(
        "{:<12} {:<20} {:<10} {:<10}",
        "Login", "Name", "TTY", "Idle"
    );
    unsafe {
        libc::setutxent();
        loop {
            let ut = libc::getutxent();
            if ut.is_null() {
                break;
            }
            if (*ut).ut_type as libc::c_int == libc::USER_PROCESS as libc::c_int {
                let user_bytes = &(*ut).ut_user;
                let mut u_len = 0;
                while u_len < user_bytes.len() && user_bytes[u_len] != 0 {
                    u_len += 1;
                }
                let u8_user: Vec<u8> = user_bytes[..u_len].iter().map(|&c| c as u8).collect();
                let username = String::from_utf8_lossy(&u8_user).into_owned();

                let line_bytes = &(*ut).ut_line;
                let mut l_len = 0;
                while l_len < line_bytes.len() && line_bytes[l_len] != 0 {
                    l_len += 1;
                }
                let u8_line: Vec<u8> = line_bytes[..l_len].iter().map(|&c| c as u8).collect();
                let line = String::from_utf8_lossy(&u8_line).into_owned();

                // Запрашиваем информацию о пользователе, чтобы выудить его Real Name (GECOS)
                let mut real_name = "???".to_string();
                let pwd = libc::getpwnam(user_bytes.as_ptr() as *const libc::c_char);
                if !pwd.is_null() {
                    let gecos = CStr::from_ptr((*pwd).pw_gecos).to_string_lossy();
                    // Традиционно поле GECOS содержит имя до первой запятой
                    if let Some(first) = gecos.split(',').next() {
                        real_name = first.to_string();
                    }
                }

                if !username.is_empty() {
                    println!(
                        "{:<12} {:<20} {:<10} {:<10}",
                        username, real_name, line, "no"
                    );
                }
            }
        }
        libc::endutxent();
    }
}
