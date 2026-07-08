fn main() {
    unsafe {
        let id = libc::gethostid();
        println!("{:08x}", id);
    }
}
