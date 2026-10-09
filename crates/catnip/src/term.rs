use std::{
    io::{self, Write},
    os::fd::AsRawFd,
    sync::atomic::{AtomicI32, Ordering},
};

use crossterm::{
    cursor::{Hide, Show},
    event::{
        DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};

/// Terminal size in cells and in pixels, as reported by the kernel.
#[derive(Clone, Copy, Debug)]
pub struct Window {
    pub cols: u32,
    pub rows: u32,
    pub width: u32,
    pub height: u32,
}

impl Window {
    pub fn cell(&self) -> (u32, u32) {
        (
            self.width / self.cols.max(1),
            self.height / self.rows.max(1),
        )
    }
}

pub fn window_size() -> io::Result<Window> {
    // SAFETY: winsize is plain data and TIOCGWINSZ only writes into it.
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if ws.ws_xpixel == 0 || ws.ws_ypixel == 0 {
        return Err(io::Error::other("terminal did not report its pixel size"));
    }
    Ok(Window {
        cols: ws.ws_col.into(),
        rows: ws.ws_row.into(),
        width: ws.ws_xpixel.into(),
        height: ws.ws_ypixel.into(),
    })
}

/// kitty, or Ghostty (which implements the kitty graphics and keyboard protocols).
pub fn supported_terminal() -> bool {
    let term = std::env::var("TERM").unwrap_or_default();
    std::env::var_os("KITTY_WINDOW_ID").is_some()
        || std::env::var("TERM_PROGRAM").is_ok_and(|p| p == "ghostty")
        || term == "xterm-kitty"
        || term == "xterm-ghostty"
}

pub fn enter() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let mut out = io::stdout();
    // REPORT_EVENT_TYPES is what gives us key *release* events, so held keys work.
    execute!(
        out,
        EnterAlternateScreen,
        Hide,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        ),
        EnableMouseCapture,
        EnableFocusChange
    )?;
    // 1016: mouse reports in pixels instead of cells. OSC 22: crosshair pointer.
    out.write_all(b"\x1b[?1016h\x1b]22;>crosshair\x1b\\")?;
    out.flush()
}

/// Safe to call more than once (normal exit and panic hook).
pub fn restore() {
    // Write to fd 1 directly: a panic on another thread must not block on the
    // stdout lock the frame loop is holding.
    // SAFETY: fd 1 stays open; ManuallyDrop keeps File from closing it.
    let mut out = std::mem::ManuallyDrop::new(unsafe {
        <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(libc::STDOUT_FILENO)
    });
    let out = &mut *out;
    let _ = out.write_all(b"\x1b_Ga=d,d=A,q=2\x1b\\\x1b[?1016l\x1b]22;<\x1b\\");
    let _ = execute!(
        out,
        DisableFocusChange,
        DisableMouseCapture,
        PopKeyboardEnhancementFlags,
        Show,
        LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
    restore_stderr();
}

static SAVED_STDERR: AtomicI32 = AtomicI32::new(-1);

/// Sends stderr to a log file while the game owns the screen, so library
/// warnings (ALSA loves printing) don't smear text over the frame.
pub fn redirect_stderr(path: &std::path::Path) -> io::Result<()> {
    let log = std::fs::File::create(path)?;
    // SAFETY: plain fd duplication; the saved fd is restored in restore_stderr.
    unsafe {
        let saved = libc::dup(libc::STDERR_FILENO);
        if saved < 0 {
            return Err(io::Error::last_os_error());
        }
        SAVED_STDERR.store(saved, Ordering::SeqCst);
        libc::dup2(log.as_raw_fd(), libc::STDERR_FILENO);
    }
    Ok(())
}

fn restore_stderr() {
    let saved = SAVED_STDERR.swap(-1, Ordering::SeqCst);
    if saved >= 0 {
        // SAFETY: `saved` came from dup() above and is closed exactly once.
        unsafe {
            libc::dup2(saved, libc::STDERR_FILENO);
            libc::close(saved);
        }
    }
}
