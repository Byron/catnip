use std::{
    collections::VecDeque,
    fs::File,
    io::{self, Write},
    mem::ManuallyDrop,
    os::windows::io::{AsRawHandle, FromRawHandle},
    sync::Mutex,
    time::{Duration, Instant},
};

use crossterm::event::Event;
use windows_sys::Win32::{
    Foundation::{HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::{Console::*, Threading::WaitForSingleObject},
};

use super::{
    Window,
    input::{Message, Parser},
    window_from_replies,
};

fn std_handle(which: STD_HANDLE) -> io::Result<HANDLE> {
    // SAFETY: GetStdHandle only looks up a process-owned handle.
    let handle = unsafe { GetStdHandle(which) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        Err(io::Error::other("a Windows console is required"))
    } else {
        Ok(handle)
    }
}

struct ConsoleModes {
    input: u32,
    output: u32,
}

impl ConsoleModes {
    fn enter() -> io::Result<Self> {
        let input = std_handle(STD_INPUT_HANDLE)?;
        let output = std_handle(STD_OUTPUT_HANDLE)?;
        let mut modes = Self {
            input: 0,
            output: 0,
        };
        // SAFETY: valid console handles and pointers to DWORDs owned here.
        unsafe {
            if GetConsoleMode(input, &mut modes.input) == 0
                || GetConsoleMode(output, &mut modes.output) == 0
            {
                // No modes have changed yet; don't run the restoring Drop.
                std::mem::forget(modes);
                return Err(io::Error::last_os_error());
            }
            let raw = (modes.input
                & !(ENABLE_LINE_INPUT
                    | ENABLE_ECHO_INPUT
                    | ENABLE_PROCESSED_INPUT
                    | ENABLE_QUICK_EDIT_MODE
                    | ENABLE_MOUSE_INPUT))
                | ENABLE_EXTENDED_FLAGS
                | ENABLE_WINDOW_INPUT
                | ENABLE_VIRTUAL_TERMINAL_INPUT;
            if SetConsoleMode(input, raw) == 0
                || SetConsoleMode(output, modes.output | ENABLE_VIRTUAL_TERMINAL_PROCESSING) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(modes)
    }
}

impl Drop for ConsoleModes {
    fn drop(&mut self) {
        // SAFETY: restore the exact flags captured before changing either mode.
        unsafe {
            if let Ok(input) = std_handle(STD_INPUT_HANDLE) {
                SetConsoleMode(input, self.input);
            }
            if let Ok(output) = std_handle(STD_OUTPUT_HANDLE) {
                SetConsoleMode(output, self.output);
            }
        }
    }
}

static MODES: Mutex<Option<ConsoleModes>> = Mutex::new(None);

pub(super) fn enable_raw_mode() -> io::Result<()> {
    let mut modes = MODES.lock().unwrap_or_else(|e| e.into_inner());
    if modes.is_none() {
        *modes = Some(ConsoleModes::enter()?);
    }
    Ok(())
}

pub(super) fn disable_raw_mode() -> io::Result<()> {
    MODES.lock().unwrap_or_else(|e| e.into_inner()).take();
    Ok(())
}

pub fn window_size() -> io::Result<Window> {
    // Startup calls this before entering the alternate screen. Temporarily put
    // input in raw VT mode so replies do not echo or wait for a newline.
    let _modes = ConsoleModes::enter()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut out = io::stdout();
    loop {
        out.write_all(b"\x1b[14t\x1b[16t\x1b[18t")?;
        out.flush()?;
        let left = deadline.saturating_duration_since(Instant::now());
        let reply = read_input_until(left.min(Duration::from_millis(100)), |bytes| {
            window_from_replies(bytes).is_some()
        });
        if let Some(window) = window_from_replies(&reply) {
            return Ok(window);
        }
        // Rustty may start its child before the first window layout, reporting
        // zero pixels initially. Retry, with a deadline, instead of guessing.
        if Instant::now() >= deadline {
            return Err(io::Error::other(
                "terminal did not report its pixel size; run in Rustty",
            ));
        }
    }
}

struct Reader {
    parser: Parser,
    events: VecDeque<Event>,
    replies: Vec<u8>,
    surrogate: Option<u16>,
    window: Option<Window>,
}

static INPUT: Mutex<Reader> = Mutex::new(Reader {
    parser: Parser::new(),
    events: VecDeque::new(),
    replies: Vec::new(),
    surrogate: None,
    window: None,
});

impl Reader {
    fn read(&mut self, timeout: Duration) -> io::Result<bool> {
        let input = std_handle(STD_INPUT_HANDLE)?;
        let millis = if timeout.is_zero() {
            0
        } else {
            timeout
                .as_millis()
                .saturating_add(1)
                .min(u128::from(u32::MAX - 1)) as u32
        };
        // SAFETY: input is a live console handle. The bounded wait also handles
        // resize/focus records, which cannot safely be polled with ReadFile.
        match unsafe { WaitForSingleObject(input, millis) } {
            WAIT_OBJECT_0 => {}
            WAIT_TIMEOUT => return Ok(false),
            _ => return Err(io::Error::last_os_error()),
        }
        let mut records = [INPUT_RECORD::default(); 128];
        let mut count = 0;
        // SAFETY: records has room for exactly the requested count. The console
        // writes count records, each tagged with the corresponding union variant.
        if unsafe {
            ReadConsoleInputW(
                input,
                records.as_mut_ptr(),
                records.len() as u32,
                &mut count,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "console input closed",
            ));
        }
        for record in &records[..count as usize] {
            match u32::from(record.EventType) {
                KEY_EVENT => {
                    // SAFETY: EventType selected this union member.
                    let key = unsafe { record.Event.KeyEvent };
                    let unit = unsafe { key.uChar.UnicodeChar };
                    // With VT input enabled, ConPTY delivers terminal sequences
                    // as Unicode key-down records, including CSI-u key releases.
                    if key.bKeyDown != 0 && unit != 0 {
                        for _ in 0..key.wRepeatCount.max(1) {
                            self.utf16(unit);
                        }
                    }
                }
                WINDOW_BUFFER_SIZE_EVENT => {
                    let size = unsafe { record.Event.WindowBufferSizeEvent.dwSize };
                    if size.X > 0 && size.Y > 0 {
                        self.events
                            .push_back(Event::Resize(size.X as u16, size.Y as u16));
                    }
                }
                FOCUS_EVENT => {
                    let focused = unsafe { record.Event.FocusEvent.bSetFocus } != 0;
                    self.events.push_back(if focused {
                        Event::FocusGained
                    } else {
                        Event::FocusLost
                    });
                }
                _ => {}
            }
            // Keep native focus/resize records in order with decoded VT keys.
            self.dispatch();
        }
        Ok(true)
    }

    fn dispatch(&mut self) {
        while let Some(message) = self.parser.next() {
            match message {
                Message::Event(event) => self.events.push_back(event),
                Message::Reply(bytes) => {
                    // In-band reports also notice font/DPI changes that leave
                    // the number of cells unchanged, unlike native resize events.
                    if let Some(window) = window_from_replies(&bytes)
                        && self.window != Some(window)
                    {
                        self.window = Some(window);
                        self.events
                            .push_back(Event::Resize(window.cols as u16, window.rows as u16));
                    }
                    self.replies.extend(bytes);
                }
            }
        }
    }

    fn utf16(&mut self, unit: u16) {
        if (0xd800..=0xdbff).contains(&unit) {
            self.surrogate = Some(unit);
            return;
        }
        let code = match self.surrogate.take() {
            Some(high) if (0xdc00..=0xdfff).contains(&unit) => {
                0x10000 + ((u32::from(high) - 0xd800) << 10) + u32::from(unit) - 0xdc00
            }
            _ => u32::from(unit),
        };
        if let Some(c) = char::from_u32(code) {
            self.parser.push(c.encode_utf8(&mut [0; 4]).as_bytes());
        }
    }
}

pub fn read_input_until(timeout: Duration, done: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let mut reader = INPUT.lock().unwrap_or_else(|e| e.into_inner());
    let mut bytes = std::mem::take(&mut reader.replies);
    let deadline = Instant::now() + timeout;
    while !done(&bytes) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || !reader.read(left).unwrap_or(false) {
            break;
        }
        bytes.append(&mut reader.replies);
    }
    bytes
}

pub fn read_event(timeout: Duration) -> io::Result<Option<Event>> {
    let mut reader = INPUT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(event) = reader.events.pop_front() {
        return Ok(Some(event));
    }
    let deadline = Instant::now() + timeout;
    loop {
        if !reader.read(deadline.saturating_duration_since(Instant::now()))? {
            return Ok(None);
        }
        if let Some(event) = reader.events.pop_front() {
            return Ok(Some(event));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
    }
}

pub(super) fn stdout() -> io::Result<ManuallyDrop<File>> {
    let handle = std_handle(STD_OUTPUT_HANDLE)?;
    // SAFETY: the process owns this handle. ManuallyDrop leaves it open.
    Ok(ManuallyDrop::new(unsafe { File::from_raw_handle(handle) }))
}

// Keep the log alive while installed as the process's standard-error handle.
// Store the borrowed previous handle as an integer so the state is Send.
static STDERR: Mutex<Option<(isize, File)>> = Mutex::new(None);

pub fn redirect_stderr(path: &std::path::Path) -> io::Result<()> {
    let log = File::create(path)?;
    let mut saved = STDERR.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: log owns a valid handle; the original handle is borrowed, never closed.
    let original = saved
        .as_ref()
        .map(|(handle, _)| *handle)
        .unwrap_or_else(|| unsafe { GetStdHandle(STD_ERROR_HANDLE) } as isize);
    if unsafe { SetStdHandle(STD_ERROR_HANDLE, log.as_raw_handle()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    *saved = Some((original, log));
    Ok(())
}

pub(super) fn restore_stderr() {
    let mut saved = STDERR.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((original, _)) = saved.as_ref() {
        // SAFETY: restore the borrowed handle before dropping the log file.
        if unsafe { SetStdHandle(STD_ERROR_HANDLE, *original as HANDLE) } != 0 {
            saved.take();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_redirection_and_restore_use_the_windows_handle() {
        const CHILD: &str = "CATNIP_TEST_STDERR_LOG";
        if let Some(path) = std::env::var_os(CHILD) {
            eprintln!("before redirect");
            redirect_stderr(std::path::Path::new(&path)).unwrap();
            eprintln!("inside log");
            restore_stderr();
            restore_stderr();
            eprintln!("after restore");
            return;
        }
        let path =
            std::env::temp_dir().join(format!("catnip-stderr-test-{}.log", std::process::id()));
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "term::sys::tests::stderr_redirection_and_restore_use_the_windows_handle",
                "--nocapture",
            ])
            .env(CHILD, &path)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        let stderr = String::from_utf8(child.stderr).unwrap();
        assert!(stderr.contains("before redirect") && stderr.contains("after restore"));
        assert!(!stderr.contains("inside log"));
        assert_eq!(std::fs::read_to_string(&path).unwrap().trim(), "inside log");
        std::fs::remove_file(path).unwrap();
    }
}
