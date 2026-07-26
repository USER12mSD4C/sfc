use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
use std::sync::atomic::{AtomicBool, Ordering};

static mut PIPE_WR: i32 = -1;
pub static GOT_CHLD: AtomicBool = AtomicBool::new(false);

extern "C" fn sigchld_handler(_: libc::c_int) {
    unsafe {
        if PIPE_WR >= 0 {
            let buf = [1u8];
            let _ = libc::write(PIPE_WR, buf.as_ptr() as *const libc::c_void, 1);
        }
    }
    GOT_CHLD.store(true, Ordering::SeqCst);
}

pub fn setup_signals() -> i32 {
    let mut fds = [0i32; 2];
    unsafe {
        libc::pipe(fds.as_mut_ptr());
    }
    unsafe {
        PIPE_WR = fds[1];
    }

    let handler = SigHandler::Handler(sigchld_handler);
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGCHLD);
    let action = SigAction::new(handler, SaFlags::SA_RESTART, mask);
    unsafe {
        sigaction(Signal::SIGCHLD, &action).unwrap();
    }

    unsafe {
        libc::signal(libc::SIGTTOU, libc::SIG_IGN);
        libc::signal(libc::SIGTTIN, libc::SIG_IGN);
        libc::signal(libc::SIGTSTP, libc::SIG_IGN);
    }

    fds[0]
}
