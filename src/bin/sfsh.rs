use sfc::sfsh::main::sfsh_main;

fn main() {
    if let Err(e) = sfsh_main() {
        eprintln!("sfshell: {}", e);
        std::process::exit(1);
    }
}
