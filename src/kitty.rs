//! Kitty graphics protocol commands. Pixel data goes through POSIX shared
//! memory (`t=s`), so only tiny escape codes travel through the pty.

use std::{
    ffi::CString,
    fs::File,
    io::{self, Write},
    os::fd::FromRawFd,
    sync::atomic::{AtomicU64, Ordering},
};

use base64::{Engine, engine::general_purpose::STANDARD};

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
    let mut file = unsafe { File::from_raw_fd(fd) };
    file.set_len(pixels.len() as u64)?;
    file.write_all(pixels)?;
    Ok(STANDARD.encode(name))
}

/// Removes shared-memory objects Kitty never consumed (e.g. after a crash mid-frame).
pub fn cleanup_shm() {
    let prefix = shm_prefix();
    if let Ok(entries) = std::fs::read_dir("/dev/shm") {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// Uploads an image without showing it; placements reference it later.
pub fn upload(out: &mut Vec<u8>, id: u32, w: u32, h: u32, rgba: &[u8]) -> io::Result<()> {
    let name = write_shm(rgba)?;
    write!(
        out,
        "\x1b_Ga=t,i={id},f=32,s={w},v={h},t=s,q=2;{name}\x1b\\"
    )
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
