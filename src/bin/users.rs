fn main() {
    let mut list = Vec::new();
    unsafe {
        libc::setutxent();

        loop {
            let ut = libc::getutxent();
            if ut.is_null() {
                break;
            }
            if (*ut).ut_type as libc::c_int == libc::USER_PROCESS as libc::c_int {
                let user_bytes = &(*ut).ut_user;

                let mut len = 0;
                while len < user_bytes.len() && user_bytes[len] != 0 {
                    len += 1;
                }

                let u8_bytes: Vec<u8> = user_bytes[..len].iter().map(|&c| c as u8).collect();
                let username = String::from_utf8_lossy(&u8_bytes).into_owned();

                if !username.is_empty() {
                    list.push(username);
                }
            }
        }
        libc::endutxent();
    }

    list.sort();
    list.dedup();
    println!("{}", list.join(" "));
}
