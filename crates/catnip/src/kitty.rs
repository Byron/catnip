//! Kitty graphics protocol commands. Pixel data goes through POSIX shared
//! memory (`t=s`), so only tiny escape codes travel through the pty.

use std::{
    ffi::CString,
    fs::File,
    io::{self, Write},
    os::fd::{AsRawFd, FromRawFd},
    sync::atomic::{AtomicU64, Ordering},
};

use base64::{Engine, engine::general_purpose::STANDARD};

use crate::gfx::Frame;

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

static SHM_COUNTER: AtomicU64 = AtomicU64::new(0);

fn shm_prefix() -> String {
    format!("catnip-{}-", std::process::id())
}

/// Copies RGBA pixels into a fresh shared-memory object. Kitty unlinks it after reading.
fn write_shm(pixels: &[u8]) -> io::Result<String> {
    let name = format!(
        "/{}{}",
        shm_prefix(),
        SHM_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let c_name = CString::new(name.clone()).expect("shm name has no NUL");
    // SAFETY: c_name is a valid C string; the returned fd is owned by `file` below.
    let fd = unsafe {
        libc::shm_open(
            c_name.as_ptr(),
            libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
            0o600,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let result = (|| {
        file.set_len(pixels.len() as u64)?;
        if !pixels.is_empty() {
            // macOS shared-memory fds do not support write(); use a shared mapping.
            // SAFETY: file is sized above; copy exactly that many bytes, then unmap.
            let mapped = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    pixels.len(),
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    file.as_raw_fd(),
                    0,
                )
            };
            if mapped == libc::MAP_FAILED {
                return Err(io::Error::last_os_error());
            }
            unsafe {
                std::ptr::copy_nonoverlapping(pixels.as_ptr(), mapped.cast(), pixels.len());
                if libc::munmap(mapped, pixels.len()) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
        }
        Ok(STANDARD.encode(name))
    })();
    if result.is_err() {
        // SAFETY: c_name still names the object we created above.
        unsafe { libc::shm_unlink(c_name.as_ptr()) };
    }
    result
}

/// Removes shared-memory objects Kitty never consumed (e.g. after a crash mid-frame).
pub fn cleanup_shm() {
    let prefix = shm_prefix();
    // macOS does not expose shared-memory objects through /dev/shm.
    for id in 0..SHM_COUNTER.load(Ordering::Relaxed) {
        let name = CString::new(format!("/{prefix}{id}")).expect("shm name has no NUL");
        // SAFETY: name is NUL-terminated and belongs to this process.
        unsafe { libc::shm_unlink(name.as_ptr()) };
    }
}

/// Uploads an image without showing it; placements reference it later.
/// Only errors are reported back (`q=1`); call `verify` afterwards to see them.
pub fn upload(out: &mut Vec<u8>, id: u32, w: u32, h: u32, rgba: &[u8]) -> io::Result<()> {
    let name = write_shm(rgba)?;
    write!(
        out,
        "\x1b_Ga=t,i={id},f=32,s={w},v={h},t=s,q=1;{name}\x1b\\"
    )
}

/// Waits for the terminal to finish everything sent so far and fails if it
/// rejected an upload, or never answers (graphics protocol unavailable, e.g.
/// inside tmux or over SSH where shared memory isn't reachable).
pub fn verify(out: &mut impl io::Write, timeout: std::time::Duration) -> Result<(), String> {
    const MARKER: u32 = 0x7f00_0001;
    // A query for a 1×1 image always gets a reply, after all earlier commands.
    write!(out, "\x1b_Ga=q,i={MARKER},s=1,v=1,f=24,t=d;AAAA\x1b\\")
        .and_then(|()| out.flush())
        .map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + timeout;
    let mut received = Vec::new();
    loop {
        for (id, message) in replies(&received) {
            if id == MARKER {
                return Ok(());
            }
            if message != "OK" {
                return Err(format!("the terminal rejected image {id}: {message}"));
            }
        }
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return Err("no reply from the terminal's graphics protocol \
                        (inside tmux, over SSH, or not kitty/Ghostty?)"
                .to_string());
        }
        let mut poll = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd; read into a stack buffer of the given size.
        let ready = unsafe { libc::poll(&mut poll, 1, left.as_millis().min(1000) as i32) };
        if ready > 0 {
            let mut chunk = [0u8; 1024];
            let n =
                unsafe { libc::read(libc::STDIN_FILENO, chunk.as_mut_ptr().cast(), chunk.len()) };
            if n > 0 {
                received.extend_from_slice(&chunk[..n as usize]);
            }
        }
    }
}

/// Complete graphics replies `ESC _G i=<id>[,…];<message> ESC \` in `bytes`.
fn replies(bytes: &[u8]) -> Vec<(u32, String)> {
    let text = String::from_utf8_lossy(bytes);
    text.split("\x1b_G")
        .skip(1)
        .filter_map(|reply| {
            let (body, _) = reply.split_once("\x1b\\")?;
            let (keys, message) = body.split_once(';')?;
            let id = keys
                .split(',')
                .find_map(|kv| kv.strip_prefix("i="))?
                .parse()
                .ok()?;
            Some((id, message.to_string()))
        })
        .collect()
}

/// Uploads an image and stretches it over the whole window.
pub fn show_fullscreen(
    out: &mut Vec<u8>,
    id: u32,
    (w, h): (u32, u32),
    rgba: &[u8],
    (cols, rows): (u32, u32),
    z: i32,
) -> io::Result<()> {
    let name = write_shm(rgba)?;
    write!(
        out,
        "\x1b[1;1H\x1b_Ga=T,i={id},p=1,f=32,s={w},v={h},t=s,c={cols},r={rows},z={z},C=1,q=2;{name}\x1b\\"
    )
}

/// Shows `src` of an uploaded image at pixel position (x, y). Re-placing the same
/// (image, placement) pair moves it without flicker.
pub fn place(
    out: &mut Vec<u8>,
    (image, placement): (u32, u32),
    src: Rect,
    (x, y): (u32, u32),
    (cell_w, cell_h): (u32, u32),
    z: i32,
) {
    let (col, off_x) = (x / cell_w, x % cell_w);
    let (row, off_y) = (y / cell_h, y % cell_h);
    let Rect { x: sx, y: sy, w, h } = src;
    let _ = write!(
        out,
        "\x1b[{};{}H\x1b_Ga=p,i={image},p={placement},x={sx},y={sy},w={w},h={h},X={off_x},Y={off_y},z={z},C=1,q=2\x1b\\",
        row + 1,
        col + 1
    );
}

pub fn delete_placement(out: &mut Vec<u8>, image: u32, placement: u32) {
    let _ = write!(out, "\x1b_Ga=d,d=i,i={image},p={placement},q=2\x1b\\");
}

/// Deletes an image and frees its data.
pub fn delete_image(out: &mut Vec<u8>, image: u32) {
    let _ = write!(out, "\x1b_Ga=d,d=I,i={image},q=2\x1b\\");
}

/// Placers for every page of a sprite atlas uploaded as consecutive ids.
pub struct AtlasPlacer {
    pages: Vec<Placer>,
}

impl AtlasPlacer {
    pub fn new(first_id: u32, pages: usize) -> Self {
        Self {
            pages: (0..pages as u32)
                .map(|i| Placer::new(first_id + i))
                .collect(),
        }
    }

    pub fn begin(&mut self, window: (u32, u32), cell: (u32, u32)) {
        self.pages.iter_mut().for_each(|p| p.begin(window, cell));
    }

    /// Places `frame` with its top-left at window pixel `at`.
    pub fn place(&mut self, out: &mut Vec<u8>, frame: Frame, at: (i32, i32), z: i32) {
        self.pages[frame.page].place(out, frame.rect, at, z);
    }

    pub fn finish(&mut self, out: &mut Vec<u8>) {
        self.pages.iter_mut().for_each(|p| p.finish(out));
    }

    pub fn count(&self) -> u32 {
        self.pages.iter().map(Placer::count).sum()
    }
}

/// Hands out placement ids for one image, frame by frame. Every visible sprite
/// is re-placed each frame using ids 1..n; ids left over from a busier previous
/// frame get deleted in `finish`.
pub struct Placer {
    image: u32,
    next: u32,
    previous: u32,
    window: (i32, i32),
    cell: (u32, u32),
}

impl Placer {
    pub fn new(image: u32) -> Self {
        Self {
            image,
            next: 1,
            previous: 0,
            window: (0, 0),
            cell: (1, 1),
        }
    }

    pub fn begin(&mut self, (width, height): (u32, u32), cell: (u32, u32)) {
        self.next = 1;
        self.window = (width as i32, height as i32);
        self.cell = (cell.0.max(1), cell.1.max(1));
    }

    /// Places `src` with its top-left at window pixel (x, y), cropping whatever
    /// falls outside the window (kitty can't place at negative positions).
    pub fn place(&mut self, out: &mut Vec<u8>, src: Rect, (x, y): (i32, i32), z: i32) {
        let (mut sx, mut sy, mut w, mut h) =
            (src.x as i32, src.y as i32, src.w as i32, src.h as i32);
        let (mut x, mut y) = (x, y);
        if x < 0 {
            sx -= x;
            w += x;
            x = 0;
        }
        if y < 0 {
            sy -= y;
            h += y;
            y = 0;
        }
        w = w.min(self.window.0 - x);
        h = h.min(self.window.1 - y);
        if w <= 0 || h <= 0 {
            return;
        }
        let rect = Rect {
            x: sx as u32,
            y: sy as u32,
            w: w as u32,
            h: h as u32,
        };
        place(
            out,
            (self.image, self.next),
            rect,
            (x as u32, y as u32),
            self.cell,
            z,
        );
        self.next += 1;
    }

    pub fn finish(&mut self, out: &mut Vec<u8>) {
        for stale in self.next..=self.previous {
            delete_placement(out, self.image, stale);
        }
        self.previous = self.next - 1;
    }

    pub fn count(&self) -> u32 {
        self.next - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_memory_contains_pixels_and_is_cleaned_up() {
        for pixels in [&b"RGBA"[..], &b""[..]] {
            let encoded = write_shm(pixels).unwrap();
            let name = CString::new(STANDARD.decode(encoded).unwrap()).unwrap();
            // SAFETY: name is NUL-terminated; the returned fd belongs to file.
            let fd = unsafe { libc::shm_open(name.as_ptr(), libc::O_RDONLY, 0) };
            assert!(fd >= 0, "{}", io::Error::last_os_error());
            let file = unsafe { File::from_raw_fd(fd) };
            // macOS rounds shared-memory size up to a page.
            assert!(file.metadata().unwrap().len() >= pixels.len() as u64);
            if !pixels.is_empty() {
                // SAFETY: map the object's full size, then unmap after reading.
                let mapped = unsafe {
                    libc::mmap(
                        std::ptr::null_mut(),
                        pixels.len(),
                        libc::PROT_READ,
                        libc::MAP_SHARED,
                        file.as_raw_fd(),
                        0,
                    )
                };
                assert_ne!(mapped, libc::MAP_FAILED, "{}", io::Error::last_os_error());
                unsafe {
                    assert_eq!(
                        std::slice::from_raw_parts(mapped.cast::<u8>(), pixels.len()),
                        pixels
                    );
                    assert_eq!(libc::munmap(mapped, pixels.len()), 0);
                }
            }
            cleanup_shm();
            assert_eq!(
                unsafe { libc::shm_open(name.as_ptr(), libc::O_RDONLY, 0) },
                -1
            );
            assert_eq!(
                io::Error::last_os_error().raw_os_error(),
                Some(libc::ENOENT)
            );
        }
    }

    #[test]
    fn placer_crops_offscreen_parts_and_deletes_stale_ids() {
        let mut placer = Placer::new(5);
        let mut out = Vec::new();
        let src = Rect {
            x: 100,
            y: 40,
            w: 30,
            h: 20,
        };
        placer.begin((200, 100), (10, 20));
        // Hanging 10 px off the left edge: the source rect loses its first 10 px.
        placer.place(&mut out, src, (-10, 0), 0);
        // Entirely off-screen: no placement at all.
        placer.place(&mut out, src, (500, 500), 0);
        placer.place(&mut out, src, (50, 50), 0);
        placer.finish(&mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("x=110,y=40,w=20,h=20"), "{text:?}");
        assert_eq!(placer.count(), 2);
        // Next frame draws one sprite: placement 2 must be deleted.
        let mut out = Vec::new();
        placer.begin((200, 100), (10, 20));
        placer.place(&mut out, src, (50, 50), 0);
        placer.finish(&mut out);
        assert!(String::from_utf8(out).unwrap().contains("a=d,d=i,i=5,p=2"));
    }

    #[test]
    fn graphics_replies_are_parsed_by_id() {
        let bytes =
            b"junk\x1b_Gi=7;OK\x1b\\\x1b_Gi=10,p=2;EINVAL:Image too large\x1b\\\x1b_Gi=3;partial";
        let got = replies(bytes);
        assert_eq!(
            got,
            vec![
                (7, "OK".to_string()),
                (10, "EINVAL:Image too large".to_string())
            ]
        );
    }

    #[test]
    fn place_splits_pixel_position_into_cell_and_offset() {
        let mut out = Vec::new();
        let src = Rect {
            x: 24,
            y: 0,
            w: 24,
            h: 24,
        };
        place(&mut out, (1, 7), src, (95, 41), (10, 20), -5);
        let s = String::from_utf8(out).unwrap();
        assert!(s.starts_with("\x1b[3;10H"), "{s:?}");
        assert!(s.contains("X=5,Y=1,"), "{s:?}");
        assert!(s.contains("i=1,p=7,x=24,y=0,w=24,h=24"), "{s:?}");
    }
}
