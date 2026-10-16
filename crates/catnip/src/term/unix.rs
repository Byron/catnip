use std::{
    fs::File,
    io,
    mem::ManuallyDrop,
    os::fd::{AsRawFd, FromRawFd},
    sync::atomic::{AtomicI32, Ordering},
    time::{Duration, Instant},
};

use crossterm::event::{self, Event};

use super::Window;

pub(super) use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

pub fn window_size() -> io::Result<Window> {
    // SAFETY: winsize is plain data and TIOCGWINSZ only writes into it.
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Window::checked(
        ws.ws_col.into(),
        ws.ws_row.into(),
        ws.ws_xpixel.into(),
        ws.ws_ypixel.into(),
    )
}

pub fn read_input_until(timeout: Duration, done: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    while !done(&bytes) {
        let left = deadline.saturating_duration_since(Instant::now());
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
        if ready < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if ready > 0 {
            let mut chunk = [0u8; 1024];
            let n =
                unsafe { libc::read(libc::STDIN_FILENO, chunk.as_mut_ptr().cast(), chunk.len()) };
            if n <= 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..n as usize]);
        }
    }
    bytes
}

pub fn read_event(timeout: Duration) -> io::Result<Option<Event>> {
    if event::poll(timeout)? {
        event::read().map(Some)
    } else {
        Ok(None)
    }
}

pub(super) fn stdout() -> io::Result<ManuallyDrop<File>> {
    // SAFETY: fd 1 belongs to the process. ManuallyDrop leaves it open.
    Ok(ManuallyDrop::new(unsafe {
        File::from_raw_fd(libc::STDOUT_FILENO)
    }))
}

static SAVED_STDERR: AtomicI32 = AtomicI32::new(-1);

pub fn redirect_stderr(path: &std::path::Path) -> io::Result<()> {
    let log = File::create(path)?;
    // SAFETY: plain fd duplication; the saved fd is restored in restore_stderr.
    unsafe {
        let saved = libc::dup(libc::STDERR_FILENO);
        if saved < 0 {
            return Err(io::Error::last_os_error());
        }
        if libc::dup2(log.as_raw_fd(), libc::STDERR_FILENO) < 0 {
            let error = io::Error::last_os_error();
            libc::close(saved);
            return Err(error);
        }
        let previous = SAVED_STDERR.swap(saved, Ordering::SeqCst);
        if previous >= 0 {
            // Preserve the original destination across repeated redirects.
            SAVED_STDERR.store(previous, Ordering::SeqCst);
            libc::close(saved);
        }
    }
    Ok(())
}

pub(super) fn restore_stderr() {
    let saved = SAVED_STDERR.swap(-1, Ordering::SeqCst);
    if saved >= 0 {
        // SAFETY: saved came from dup() above and is closed exactly once.
        unsafe {
            libc::dup2(saved, libc::STDERR_FILENO);
            libc::close(saved);
        }
    }
}
