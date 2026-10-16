use std::io::{self, Write};

use crossterm::{
    Command,
    cursor::{Hide, Show},
    event::{
        DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};

struct Ansi<T>(T);

impl<T: Command> std::fmt::Display for Ansi<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.write_ansi(f)
    }
}

#[cfg(any(windows, test))]
mod input;
#[cfg(unix)]
#[path = "term/unix.rs"]
mod sys;
#[cfg(windows)]
#[path = "term/windows.rs"]
mod sys;

/// Read a gameplay event, waiting at most `timeout`. Use this instead of
/// crossterm's event reader: Windows needs the terminal's VT input stream.
pub use sys::read_event;
/// Read terminal replies until `done` or the timeout. Call in raw mode; do not
/// read stdin concurrently. On Windows, gameplay events are retained for read_event.
pub use sys::read_input_until;
/// Send stderr to a log while the game owns the screen.
pub use sys::redirect_stderr;
/// Terminal size in cells and pixels. Windows queries the terminal over VT.
pub use sys::window_size;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub cols: u32,
    pub rows: u32,
    pub width: u32,
    pub height: u32,
}

impl Window {
    pub fn cell(&self) -> (u32, u32) {
        (
            (self.width / self.cols.max(1)).max(1),
            (self.height / self.rows.max(1)).max(1),
        )
    }

    fn checked(cols: u32, rows: u32, width: u32, height: u32) -> io::Result<Self> {
        if cols == 0
            || rows == 0
            || cols > u16::MAX.into()
            || rows > u16::MAX.into()
            || width < cols
            || height < rows
        {
            return Err(io::Error::other(
                "terminal did not report a usable pixel size",
            ));
        }
        Ok(Self {
            cols,
            rows,
            width,
            height,
        })
    }
}

/// Terminals that implement kitty graphics and keyboard protocols well enough
/// for real-time sprites. WezTerm mis-draws sub-cell offsets and erased cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminal {
    Kitty,
    Ghostty,
    Rustty,
}

impl Terminal {
    pub fn detect() -> Option<Self> {
        let term = std::env::var("TERM").unwrap_or_default();
        let program = std::env::var("TERM_PROGRAM").unwrap_or_default();
        // Rustty also sets TERM_PROGRAM=ghostty for compatibility.
        if program == "rustty" || std::env::var_os("RUSTTY_RESOURCES_DIR").is_some() {
            Some(Self::Rustty)
        } else if std::env::var_os("KITTY_WINDOW_ID").is_some() || term == "xterm-kitty" {
            Some(Self::Kitty)
        } else if program == "ghostty" || term == "xterm-ghostty" {
            Some(Self::Ghostty)
        } else {
            None
        }
    }
}

pub fn supported_terminal() -> bool {
    Terminal::detect().is_some()
}

const KEYBOARD_FLAGS: KeyboardEnhancementFlags = KeyboardEnhancementFlags::from_bits_retain(
    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES.bits()
        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES.bits()
        | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES.bits(),
);

/// Check the enabled keyboard flags after `enter`. A device-attributes query
/// marks the end of the replies even when keyboard enhancement is unsupported.
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
        csi_replies(bytes, b'c').any(|params| params.starts_with('?'))
    });
    keyboard_flags_active(&reply)
}

fn keyboard_flags_active(reply: &[u8]) -> bool {
    csi_replies(reply, b'u').any(|params| {
        params
            .strip_prefix('?')
            .and_then(|s| s.parse::<u8>().ok())
            .is_some_and(|flags| flags & KEYBOARD_FLAGS.bits() == KEYBOARD_FLAGS.bits())
    })
}

/// Complete CSI reports with the requested final byte, ignoring partial replies.
fn csi_replies(bytes: &[u8], final_byte: u8) -> impl Iterator<Item = &str> {
    bytes
        .windows(2)
        .enumerate()
        .filter_map(move |(index, prefix)| {
            if prefix != b"\x1b[" {
                return None;
            }
            let body = &bytes[index + 2..];
            let end = body.iter().position(|b| (0x40..=0x7e).contains(b))?;
            if body[end] != final_byte {
                return None;
            }
            std::str::from_utf8(&body[..end]).ok()
        })
}

#[cfg(any(windows, test))]
fn window_from_replies(bytes: &[u8]) -> Option<Window> {
    let (mut cells, mut pixels, mut cell_pixels, mut in_band) = (None, None, None, None);
    for params in csi_replies(bytes, b't') {
        let Ok(values) = params
            .split(';')
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()
        else {
            continue;
        };
        match values.as_slice() {
            [4, height, width] => pixels = Some((*width, *height)),
            [6, height, width] => cell_pixels = Some((*width, *height)),
            [8, rows, cols] => cells = Some((*cols, *rows)),
            [48, rows, cols, height, width] => {
                in_band = Window::checked(*cols, *rows, *width, *height).ok()
            }
            _ => {}
        }
    }
    if in_band.is_some() {
        return in_band;
    }
    let (cols, rows) = cells?;
    if let Some((width, height)) = pixels
        && let Ok(window) = Window::checked(cols, rows, width, height)
    {
        return Some(window);
    }
    let (cell_w, cell_h) = cell_pixels?;
    Window::checked(
        cols,
        rows,
        cols.checked_mul(cell_w)?,
        rows.checked_mul(cell_h)?,
    )
    .ok()
}

pub fn enter() -> io::Result<()> {
    sys::enable_raw_mode()?;
    let mut out = io::stdout();
    // Format ANSI explicitly: crossterm's Windows execute path rejects kitty
    // keyboard flags and switches mouse input to cell coordinates.
    write!(
        out,
        "{}{}{}{}{}",
        Ansi(EnterAlternateScreen),
        Ansi(Hide),
        Ansi(PushKeyboardEnhancementFlags(KEYBOARD_FLAGS)),
        Ansi(EnableMouseCapture),
        Ansi(EnableFocusChange),
    )?;
    if matches!(
        Terminal::detect(),
        Some(Terminal::Ghostty | Terminal::Rustty)
    ) {
        // XTSHIFTESCAPE: Shift is gameplay input, not a mouse-selection override.
        out.write_all(b"\x1b[>1s")?;
    }
    // Mouse coordinates in pixels; crosshair pointer. Ghostty ignores OSC 22's
    // kitty-specific push variant, so use the plain form.
    out.write_all(b"\x1b[?1016h\x1b]22;crosshair\x1b\\")?;
    #[cfg(windows)]
    out.write_all(b"\x1b[?2048h")?;
    out.flush()
}

/// Safe to call more than once (normal exit and panic hook).
pub fn restore() {
    // Bypass the stdout lock: a panic on an audio thread must not block on the
    // lock held by the frame loop. The borrowed OS handle is never closed here.
    if let Ok(mut out) = sys::stdout() {
        #[cfg(windows)]
        let _ = out.write_all(b"\x1b[?2048l");
        if matches!(
            Terminal::detect(),
            Some(Terminal::Ghostty | Terminal::Rustty)
        ) {
            let _ = out.write_all(b"\x1b[>0s");
        }
        let _ =
            out.write_all(b"\x1b[?2026l\x1b_Ga=d,d=A,q=2\x1b\\\x1b[?1016l\x1b]22;default\x1b\\");
        let _ = write!(
            out,
            "{}{}{}{}{}",
            Ansi(DisableFocusChange),
            Ansi(DisableMouseCapture),
            Ansi(PopKeyboardEnhancementFlags),
            Ansi(Show),
            Ansi(LeaveAlternateScreen),
        );
    }
    let _ = sys::disable_raw_mode();
    sys::restore_stderr();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_check_requires_releases_for_all_keys() {
        assert!(keyboard_flags_active(b"\x1b[?11u\x1b[?62;22c"));
        assert!(keyboard_flags_active(b"\xff\x1b[?11u\xfe\x1b[?62;22c"));
        assert!(keyboard_flags_active(b"\x1b[?31u"));
        for reply in [
            &b"\x1b[?0u"[..],
            b"\x1b[?1u",
            b"\x1b[?3u",
            b"\x1b[?11",
            b"\x1b[?62c\x1b[117u",
        ] {
            assert!(!keyboard_flags_active(reply), "{reply:?}");
        }
    }

    #[test]
    fn window_rejects_missing_or_invalid_metrics() {
        assert!(Window::checked(80, 24, 800, 480).is_ok());
        for values in [
            (0, 24, 800, 480),
            (80, 0, 800, 480),
            (80, 24, 0, 0),
            (80, 24, 79, 480),
        ] {
            assert!(Window::checked(values.0, values.1, values.2, values.3).is_err());
        }
    }

    #[test]
    fn windows_size_reports_use_real_pixels_and_validate_replies() {
        let expected = Window {
            cols: 99,
            rows: 35,
            width: 990,
            height: 665,
        };
        assert_eq!(
            window_from_replies(b"\x1b[4;665;990t\x1b[8;35;99t"),
            Some(expected)
        );
        assert_eq!(
            window_from_replies(b"\x1b[8;35;99t\x1b[6;19;10t"),
            Some(expected)
        );
        assert_eq!(
            window_from_replies(b"\x1b[48;35;99;665;990t"),
            Some(expected)
        );
        for bytes in [
            &b"\x1b[4;0;0t\x1b[8;24;80t"[..],
            b"\x1b[48;35;99;665;990",
            b"\x1b[6;4294967295;10t\x1b[8;35;99t",
        ] {
            assert_eq!(window_from_replies(bytes), None);
        }
    }
}
