fn main() {
    let mut uts: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut uts) } == 0 {
        let machine = unsafe { std::ffi::CStr::from_ptr(uts.machine.as_ptr()) };
        println!("{}", machine.to_string_lossy());
    } else {
        eprintln!("arch: failed to get system architecture");
        std::process::exit(1);
    }
}
