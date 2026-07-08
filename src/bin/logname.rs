use std::ffi::CStr;

fn main() {
    unsafe {
        let ptr = libc::getlogin();
        if !ptr.is_null() {
            println!("{}", CStr::from_ptr(ptr).to_string_lossy());
        } else {
            eprintln!("logname: no login name");
            std::process::exit(1);
        }
    }
}
