fn main() {
    let num_cpus = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
    if num_cpus > 0 {
        println!("{}", num_cpus);
    } else {
        println!("[FAILED]");
    }
}
