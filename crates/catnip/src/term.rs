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

/// Terminals that implement the kitty graphics and keyboard protocols well
/// enough for real-time sprites. (WezTerm implements both but mis-draws
/// images at sub-cell offsets and erased cells over images, so it's out.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminal {
    Kitty,
    Ghostty,
}

impl Terminal {
    pub fn detect() -> Option<Self> {
        let term = std::env::var("TERM").unwrap_or_default();
        let program = std::env::var("TERM_PROGRAM").unwrap_or_default();
        if std::env::var_os("KITTY_WINDOW_ID").is_some() || term == "xterm-kitty" {
            Some(Terminal::Kitty)
        } else if program == "ghostty" || term == "xterm-ghostty" {
            Some(Terminal::Ghostty)
        } else {
            None
        }
    }
}

pub fn supported_terminal() -> bool {
    Terminal::detect().is_some()
}

/// Whether the kitty keyboard protocol is active (key release events). Asks
/// for the current flags, then sends a device-attributes query that every
/// terminal answers: a flags reply before it means yes. Call after `enter`.
pub fn keyboard_protocol_active(timeout: std::time::Duration) -> bool {
    let mut out = io::stdout();
    if out
        .write_all(b"\x1b[?u\x1b[c")
        .and_then(|()| out.flush())
        .is_err()
    {
        return false;
    }
    let reply = read_input_until(timeout, |bytes| {
        // The device-attributes reply ends with `c`: ESC [ ? … c
        String::from_utf8_lossy(bytes).contains('c')
    });
    let text = String::from_utf8_lossy(&reply);
    let flags = text
        .find("\x1b[?")
        .and_then(|i| text[i..].find('u').map(|j| (i, j)));
    let attributes = text.find('c');
    match (flags, attributes) {
        (Some((i, j)), Some(c)) => i + j < c,
        _ => false,
    }
}

/// Reads raw input until `done` says the bytes so far are enough, or the
/// timeout passes. Only for start-up queries, before the event loop runs.
pub fn read_input_until(timeout: std::time::Duration, done: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let deadline = std::time::Instant::now() + timeout;
    let mut bytes = Vec::new();
    while !done(&bytes) {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            break;
        }
        let mut poll = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd; read into a stack buffer of its size.
        let ready = unsafe { libc::poll(&mut poll, 1, left.as_millis().min(1000) as i32) };
        if ready > 0 {
            let mut chunk = [0u8; 512];
            let n =
                unsafe { libc::read(libc::STDIN_FILENO, chunk.as_mut_ptr().cast(), chunk.len()) };
            if n > 0 {
                bytes.extend_from_slice(&chunk[..n as usize]);
            }
        }
    }
    bytes
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
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        ),
        EnableMouseCapture,
        EnableFocusChange
    )?;
    if Terminal::detect() == Some(Terminal::Ghostty) {
        // XTSHIFTESCAPE: Shift is gameplay input, not a mouse-selection override.
        out.write_all(b"\x1b[>1s")?;
    }
    // 1016: mouse reports in pixels instead of cells. OSC 22: crosshair pointer.
    // The plain OSC 22 form; Ghostty ignores kitty's push (`>`) variant.
    out.write_all(b"\x1b[?1016h\x1b]22;crosshair\x1b\\")?;
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
    if Terminal::detect() == Some(Terminal::Ghostty) {
        let _ = out.write_all(b"\x1b[>0s");
    }
    let _ = out.write_all(b"\x1b_Ga=d,d=A,q=2\x1b\\\x1b[?1016l\x1b]22;default\x1b\\");
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
