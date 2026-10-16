//! The VT input subset used by Catnip. Crossterm only exposes its VT parser on
//! Unix; its Windows reader turns these sequences into individual key presses.

use std::collections::VecDeque;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode, MouseButton, MouseEvent,
    MouseEventKind,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Message {
    Event(Event),
    Reply(Vec<u8>),
}

#[derive(Clone, Copy)]
enum State {
    Ground,
    Escape,
    Csi,
    Ss3,
    String { osc: bool, escape: bool },
}

pub(super) struct Parser {
    state: State,
    bytes: Vec<u8>,
    overflow: bool,
    alt: bool,
    messages: VecDeque<Message>,
}

impl Parser {
    pub const fn new() -> Self {
        Self {
            state: State::Ground,
            bytes: Vec::new(),
            overflow: false,
            alt: false,
            messages: VecDeque::new(),
        }
    }

    pub fn next(&mut self) -> Option<Message> {
        self.messages.pop_front()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.push_byte(byte);
        }
    }

    fn push_byte(&mut self, byte: u8) {
        if matches!(self.state, State::Ground) {
            if byte == 0x1b {
                self.bytes.clear();
                self.bytes.push(byte);
                self.state = State::Escape;
                return;
            }
            self.bytes.push(byte);
            match std::str::from_utf8(&self.bytes) {
                Ok(text) => {
                    if let Some(c) = text.chars().next() {
                        let mut key = plain_key(c);
                        if self.alt {
                            key.modifiers |= KeyModifiers::ALT;
                        }
                        self.messages.push_back(Message::Event(Event::Key(key)));
                    }
                }
                Err(error) if error.error_len().is_none() && self.bytes.len() < 4 => return,
                Err(_) => {}
            }
            self.bytes.clear();
            self.alt = false;
            return;
        }

        // Discard oversized control strings through their terminator. Their
        // trailing text must never become gameplay keys (notably Q / Escape).
        if self.bytes.len() < 4096 {
            self.bytes.push(byte);
        } else {
            self.overflow = true;
        }
        match self.state {
            State::Escape => match byte {
                b'[' => self.state = State::Csi,
                b'O' => self.state = State::Ss3,
                b'_' | b']' | b'P' | b'^' => {
                    self.state = State::String {
                        osc: byte == b']',
                        escape: false,
                    };
                }
                _ => {
                    self.reset();
                    self.alt = byte != 0x1b;
                    self.push_byte(byte);
                }
            },
            State::Csi => {
                if byte == 0x1b {
                    self.reset();
                    self.push_byte(byte);
                } else if (0x40..=0x7e).contains(&byte) {
                    if !self.overflow {
                        let event = std::str::from_utf8(&self.bytes[2..self.bytes.len() - 1])
                            .ok()
                            .and_then(|params| csi_event(params, byte));
                        self.messages.push_back(match event {
                            Some(event) => Message::Event(event),
                            None => Message::Reply(self.bytes.clone()),
                        });
                    }
                    self.reset();
                } else if !(0x20..=0x3f).contains(&byte) {
                    self.reset();
                }
            }
            State::Ss3 => {
                if let Some(code) = letter_key(byte) {
                    self.messages
                        .push_back(Message::Event(Event::Key(code.into())));
                }
                self.reset();
            }
            State::String { osc, escape } => {
                if (escape && byte == b'\\') || (osc && byte == 7) {
                    if !self.overflow {
                        self.messages.push_back(Message::Reply(self.bytes.clone()));
                    }
                    self.reset();
                } else {
                    self.state = State::String {
                        osc,
                        escape: byte == 0x1b,
                    };
                }
            }
            State::Ground => unreachable!(),
        }
    }

    fn reset(&mut self) {
        self.state = State::Ground;
        self.bytes.clear();
        self.overflow = false;
        self.alt = false;
    }
}

fn plain_key(c: char) -> KeyEvent {
    let code = match c {
        '\r' => KeyCode::Enter,
        '\t' => KeyCode::Tab,
        '\x1b' => KeyCode::Esc,
        '\x7f' | '\x08' => KeyCode::Backspace,
        '\x01'..='\x1a' => {
            return KeyEvent::new(
                KeyCode::Char(char::from(c as u8 - 1 + b'a')),
                KeyModifiers::CONTROL,
            );
        }
        _ => KeyCode::Char(c),
    };
    KeyEvent::new(
        code,
        if c.is_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        },
    )
}

fn csi_event(params: &str, final_byte: u8) -> Option<Event> {
    match (params, final_byte) {
        ("", b'I') => return Some(Event::FocusGained),
        ("", b'O') => return Some(Event::FocusLost),
        (_, b'M' | b'm') if params.starts_with('<') => return mouse(params, final_byte),
        _ => {}
    }
    let mut fields = params.split(';');
    let first = fields.next()?;
    let code = match final_byte {
        b'u' => kitty_key(first.split(':').next()?.parse().ok()?)?,
        b'~' => match first {
            "1" | "7" => KeyCode::Home,
            "2" => KeyCode::Insert,
            "3" => KeyCode::Delete,
            "4" | "8" => KeyCode::End,
            "5" => KeyCode::PageUp,
            "6" => KeyCode::PageDown,
            "11" => KeyCode::F(1),
            "12" => KeyCode::F(2),
            "13" => KeyCode::F(3),
            "14" => KeyCode::F(4),
            "15" => KeyCode::F(5),
            "17" => KeyCode::F(6),
            "18" => KeyCode::F(7),
            "19" => KeyCode::F(8),
            "20" => KeyCode::F(9),
            "21" => KeyCode::F(10),
            "23" => KeyCode::F(11),
            "24" => KeyCode::F(12),
            _ => return None,
        },
        other if first.is_empty() || first == "1" => letter_key(other)?,
        _ => return None,
    };
    let mut detail = fields.next().unwrap_or("1").split(':');
    let mask = detail.next()?.parse::<u8>().ok()?.checked_sub(1)?;
    let kind = match detail.next().unwrap_or("1") {
        "1" => KeyEventKind::Press,
        "2" => KeyEventKind::Repeat,
        "3" => KeyEventKind::Release,
        _ => return None,
    };
    let mut modifiers = KeyModifiers::NONE;
    for (bit, flag) in [
        (1, KeyModifiers::SHIFT),
        (2, KeyModifiers::ALT),
        (4, KeyModifiers::CONTROL),
        (8, KeyModifiers::SUPER),
        (16, KeyModifiers::HYPER),
        (32, KeyModifiers::META),
    ] {
        if mask & bit != 0 {
            modifiers |= flag;
        }
    }
    Some(Event::Key(KeyEvent::new_with_kind(code, modifiers, kind)))
}

fn letter_key(byte: u8) -> Option<KeyCode> {
    Some(match byte {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'P'..=b'S' => KeyCode::F(byte - b'P' + 1),
        _ => return None,
    })
}

fn kitty_key(code: u32) -> Option<KeyCode> {
    Some(match code {
        27 => KeyCode::Esc,
        13 => KeyCode::Enter,
        9 => KeyCode::Tab,
        127 => KeyCode::Backspace,
        57348 | 57425 => KeyCode::Insert,
        57349 | 57426 => KeyCode::Delete,
        57350 | 57417 => KeyCode::Left,
        57351 | 57418 => KeyCode::Right,
        57352 | 57419 => KeyCode::Up,
        57353 | 57420 => KeyCode::Down,
        57354 | 57421 => KeyCode::PageUp,
        57355 | 57422 => KeyCode::PageDown,
        57356 | 57423 => KeyCode::Home,
        57357 | 57424 => KeyCode::End,
        57364..=57398 => KeyCode::F((code - 57364 + 1) as u8),
        57399..=57408 => KeyCode::Char(char::from(b'0' + (code - 57399) as u8)),
        57414 => KeyCode::Enter,
        57441 => KeyCode::Modifier(ModifierKeyCode::LeftShift),
        57447 => KeyCode::Modifier(ModifierKeyCode::RightShift),
        57442 => KeyCode::Modifier(ModifierKeyCode::LeftControl),
        57448 => KeyCode::Modifier(ModifierKeyCode::RightControl),
        57443 => KeyCode::Modifier(ModifierKeyCode::LeftAlt),
        57449 => KeyCode::Modifier(ModifierKeyCode::RightAlt),
        57344..=63743 => return None,
        _ => KeyCode::Char(char::from_u32(code)?),
    })
}

fn mouse(params: &str, final_byte: u8) -> Option<Event> {
    let fields: Vec<u16> = params
        .strip_prefix('<')?
        .split(';')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let [button, x, y] = fields.as_slice() else {
        return None;
    };
    let mut modifiers = KeyModifiers::NONE;
    if button & 4 != 0 {
        modifiers |= KeyModifiers::SHIFT;
    }
    if button & 8 != 0 {
        modifiers |= KeyModifiers::ALT;
    }
    if button & 16 != 0 {
        modifiers |= KeyModifiers::CONTROL;
    }
    let kind = if button & 64 != 0 {
        match button & 3 {
            0 => MouseEventKind::ScrollUp,
            1 => MouseEventKind::ScrollDown,
            2 => MouseEventKind::ScrollLeft,
            _ => MouseEventKind::ScrollRight,
        }
    } else if button & 3 == 3 && button & 32 != 0 {
        MouseEventKind::Moved
    } else {
        let which = match button & 3 {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            _ => return None,
        };
        if final_byte == b'm' {
            MouseEventKind::Up(which)
        } else if button & 32 != 0 {
            MouseEventKind::Drag(which)
        } else {
            MouseEventKind::Down(which)
        }
    };
    Some(Event::Mouse(MouseEvent {
        kind,
        modifiers,
        // Match crossterm's coordinates; the game restores the original pixels.
        column: x.wrapping_sub(1),
        row: y.wrapping_sub(1),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(bytes: &[u8], chunk: usize) -> Vec<Message> {
        let mut parser = Parser::new();
        for part in bytes.chunks(chunk) {
            parser.push(part);
        }
        std::iter::from_fn(|| parser.next()).collect()
    }

    #[test]
    fn fragmented_keyboard_events_keep_releases_repeats_and_modifiers() {
        let bytes = b"\x1b[119u\x1b[119;1:2u\x1b[119;1:3u\x1b[1;5:3A\x1b[57441;2u\x1b[57447;1:3u";
        let expected = [
            (KeyCode::Char('w'), KeyModifiers::NONE, KeyEventKind::Press),
            (KeyCode::Char('w'), KeyModifiers::NONE, KeyEventKind::Repeat),
            (
                KeyCode::Char('w'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            (KeyCode::Up, KeyModifiers::CONTROL, KeyEventKind::Release),
            (
                KeyCode::Modifier(ModifierKeyCode::LeftShift),
                KeyModifiers::SHIFT,
                KeyEventKind::Press,
            ),
            (
                KeyCode::Modifier(ModifierKeyCode::RightShift),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
        ]
        .map(|(code, modifiers, kind)| {
            Message::Event(Event::Key(KeyEvent::new_with_kind(code, modifiers, kind)))
        });
        for chunk in 1..=bytes.len() {
            assert_eq!(parse(bytes, chunk), expected);
        }
    }

    #[test]
    fn pixel_mouse_keeps_edges_buttons_motion_and_wheel() {
        let cases = [
            ("<0;0;0M", MouseEventKind::Down(MouseButton::Left), 0, 0),
            (
                "<0;1200;750m",
                MouseEventKind::Up(MouseButton::Left),
                1200,
                750,
            ),
            ("<1;1;2M", MouseEventKind::Down(MouseButton::Middle), 1, 2),
            ("<2;1;2m", MouseEventKind::Up(MouseButton::Right), 1, 2),
            ("<32;3;4M", MouseEventKind::Drag(MouseButton::Left), 3, 4),
            ("<35;3;4M", MouseEventKind::Moved, 3, 4),
            ("<65;10;20M", MouseEventKind::ScrollDown, 10, 20),
        ];
        for (sequence, kind, x, y) in cases {
            let events = parse(format!("\x1b[{sequence}").as_bytes(), 1);
            let [Message::Event(Event::Mouse(event))] = events.as_slice() else {
                panic!("{events:?}")
            };
            assert_eq!(event.kind, kind);
            assert_eq!(
                (event.column.wrapping_add(1), event.row.wrapping_add(1)),
                (x, y)
            );
        }
    }

    #[test]
    fn terminal_replies_never_become_gameplay_keys() {
        let bytes = b"\x1b[?11u\x1b_Gi=100;ENOENT: q\x1b\\\x1b[4;600;800t\x1b[O\x1b[I\x1b[113u";
        let expected = vec![
            Message::Reply(b"\x1b[?11u".to_vec()),
            Message::Reply(b"\x1b_Gi=100;ENOENT: q\x1b\\".to_vec()),
            Message::Reply(b"\x1b[4;600;800t".to_vec()),
            Message::Event(Event::FocusLost),
            Message::Event(Event::FocusGained),
            Message::Event(Event::Key(KeyCode::Char('q').into())),
        ];
        for chunk in 1..=bytes.len() {
            assert_eq!(parse(bytes, chunk), expected);
        }
    }

    #[test]
    fn oversized_and_malformed_sequences_resynchronize() {
        let mut bytes = b"\x1b_G".to_vec();
        bytes.extend(std::iter::repeat_n(b'q', 5000));
        bytes.extend_from_slice(b"\x1b\\\x1b[999999999999999u\x1b[0;0u\x1b[broken\x1b[119u");
        let messages = parse(&bytes, 1);
        assert!(messages.contains(&Message::Event(Event::Key(KeyCode::Char('w').into()))));
        assert!(!messages.contains(&Message::Event(Event::Key(KeyCode::Char('q').into()))));
    }
}
