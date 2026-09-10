//! Last-resort terminal restoration for abnormal exits.
//!
//! The normal shutdown path and the panic hook both put the terminal back,
//! but a process that dies from a signal — `kill <pid>` from another tab
//! after a hang, a SIGHUP from the emulator — runs neither. The parent shell
//! then inherits raw mode, mouse reporting, bracketed paste and the kitty
//! keyboard protocol: every mouse move becomes `zsh: command not found:
//! 35;57;1M`, output staircases because ONLCR is off, and Ctrl+C arrives as
//! `^[[99;5u` instead of SIGINT.
//!
//! [`install`] captures the cooked termios before raw mode is enabled and
//! registers async-signal-safe handlers that write the reset sequences
//! straight to the tty, restore that termios, and re-raise the signal with
//! its default disposition so the exit status still reflects the kill.

#[cfg(unix)]
mod imp {
    use std::sync::OnceLock;

    /// `(fd, termios)` captured before raw mode; `None` if no fd is a tty.
    static SAVED_TERMIOS: OnceLock<(libc::c_int, libc::termios)> = OnceLock::new();

    /// Everything `main` enables, undone in reverse order: pop the kitty
    /// keyboard flags, mouse capture (all five modes crossterm sets),
    /// bracketed paste, focus reporting, SGR reset, show cursor, leave the
    /// alternate screen. Popping an empty kitty stack and clearing unset
    /// modes are no-ops, so this is safe however far startup got.
    const RESET: &[u8] = b"\x1b[<u\
        \x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l\
        \x1b[?2004l\x1b[?1004l\x1b[0m\x1b[?25h\x1b[?1049l";

    extern "C" fn on_fatal_signal(sig: libc::c_int) {
        // Only async-signal-safe calls here: write(2), tcsetattr(3),
        // sigaction(2), raise(3). No allocation, no locks, no logging.
        unsafe {
            let _ = libc::write(libc::STDERR_FILENO, RESET.as_ptr().cast(), RESET.len());
            if let Some((fd, termios)) = SAVED_TERMIOS.get() {
                let _ = libc::tcsetattr(*fd, libc::TCSANOW, termios);
            }
            let mut dfl: libc::sigaction = std::mem::zeroed();
            dfl.sa_sigaction = libc::SIG_DFL;
            libc::sigemptyset(&mut dfl.sa_mask);
            libc::sigaction(sig, &dfl, std::ptr::null_mut());
            libc::raise(sig);
        }
    }

    /// Call once, before raw mode is enabled (i.e. before `ratatui::init`).
    pub fn install() {
        let tty_fd = [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO]
            .into_iter()
            .find(|&fd| unsafe { libc::isatty(fd) } == 1);
        if let Some(fd) = tty_fd {
            let mut termios: libc::termios = unsafe { std::mem::zeroed() };
            if unsafe { libc::tcgetattr(fd, &mut termios) } == 0 {
                let _ = SAVED_TERMIOS.set((fd, termios));
            }
        }
        // SIGINT/SIGQUIT only reach us via `kill`: raw mode clears ISIG, so
        // the keyboard never generates them while the TUI is up.
        for sig in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT, libc::SIGQUIT] {
            unsafe {
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction =
                    on_fatal_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
                libc::sigemptyset(&mut sa.sa_mask);
                sa.sa_flags = 0;
                libc::sigaction(sig, &sa, std::ptr::null_mut());
            }
        }
    }
}

#[cfg(not(unix))]
mod imp {
    pub fn install() {}
}

pub use imp::install;
