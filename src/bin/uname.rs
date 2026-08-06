use std::io::{self, Write};

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut show_all = false;
    let mut show_sysname = false;
    let mut show_nodename = false;
    let mut show_release = false;
    let mut show_version = false;
    let mut show_machine = false;
    let mut show_processor = false;
    let mut show_hardware = false;
    let mut show_os = false;

    for arg in args {
        if arg.starts_with('-') && arg.len() > 1 {
            if arg.starts_with("--") {
                match arg.as_str() {
                    "--all" => show_all = true,
                    "--kernel-name" => show_sysname = true,
                    "--nodename" => show_nodename = true,
                    "--kernel-release" => show_release = true,
                    "--kernel-version" => show_version = true,
                    "--machine" => show_machine = true,
                    "--processor" => show_processor = true,
                    "--hardware-platform" => show_hardware = true,
                    "--operating-system" => show_os = true,
                    _ => {
                        eprintln!("uname: unrecognized option '{}'", arg);
                        std::process::exit(1);
                    }
                }
            } else {
                for c in arg.chars().skip(1) {
                    match c {
                        'a' => show_all = true,
                        's' => show_sysname = true,
                        'n' => show_nodename = true,
                        'r' => show_release = true,
                        'v' => show_version = true,
                        'm' => show_machine = true,
                        'p' => show_processor = true,
                        'i' => show_hardware = true,
                        'o' => show_os = true,
                        _ => {
                            eprintln!("uname: invalid option -- '{}'", c);
                            std::process::exit(1);
                        }
                    }
                }
            }
        } else {
            eprintln!("uname: extra operand '{}'", arg);
            std::process::exit(1);
        }
    }

    let mut any_set = show_sysname
        || show_nodename
        || show_release
        || show_version
        || show_machine
        || show_processor
        || show_hardware
        || show_os;
    if show_all {
        show_sysname = true;
        show_nodename = true;
        show_release = true;
        show_version = true;
        show_machine = true;
        show_processor = true;
        show_hardware = true;
        show_os = true;
        any_set = true;
    }

    if !any_set {
        show_sysname = true;
    }

    let mut uts = unsafe { std::mem::zeroed::<libc::utsname>() };
    if unsafe { libc::uname(&mut uts) } < 0 {
        return Err(io::Error::last_os_error());
    }

    let sysname = unsafe { std::ffi::CStr::from_ptr(uts.sysname.as_ptr()) }.to_string_lossy();
    let nodename = unsafe { std::ffi::CStr::from_ptr(uts.nodename.as_ptr()) }.to_string_lossy();
    let release = unsafe { std::ffi::CStr::from_ptr(uts.release.as_ptr()) }.to_string_lossy();
    let version = unsafe { std::ffi::CStr::from_ptr(uts.version.as_ptr()) }.to_string_lossy();
    let machine = unsafe { std::ffi::CStr::from_ptr(uts.machine.as_ptr()) }.to_string_lossy();

    let processor = "unknown";
    let hardware = "unknown";
    let os = "GNU/Linux";

    let mut output = Vec::new();

    if show_sysname {
        output.push(sysname.into_owned());
    }
    if show_nodename {
        output.push(nodename.into_owned());
    }
    if show_release {
        output.push(release.into_owned());
    }
    if show_version {
        output.push(version.into_owned());
    }
    if show_machine {
        output.push(machine.into_owned());
    }

    if show_processor {
        if !(show_all && processor == "unknown") {
            output.push(processor.to_string());
        }
    }
    if show_hardware {
        if !(show_all && hardware == "unknown") {
            output.push(hardware.to_string());
        }
    }
    if show_os {
        output.push(os.to_string());
    }

    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{}", output.join(" "))?;

    Ok(())
}
