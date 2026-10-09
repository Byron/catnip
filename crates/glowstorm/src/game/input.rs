//! What the player pressed this frame, and the visible part of the world.

use std::collections::HashSet;

use super::*;

#[derive(Default)]
pub struct Input {
    pub held: HashSet<KeyCode>,
    pub pressed: Vec<KeyCode>,
    /// Mouse position in window pixels, once the mouse has moved.
    pub mouse: Option<V2>,
    pub fire: bool,
    pub clicked: bool,
    pub dash: bool,
    /// The mouse moved this frame (hover only follows real movement).
    pub mouse_moved: bool,
}

impl Input {
    pub(super) fn down(&self, keys: &[KeyCode]) -> bool {
        keys.iter().any(|k| self.held.contains(k))
    }

    pub(super) fn hit(&self, keys: &[KeyCode]) -> bool {
        keys.iter().any(|k| self.pressed.contains(k))
    }

    pub fn end_frame(&mut self) {
        self.pressed.clear();
        self.clicked = false;
        self.dash = false;
        self.mouse_moved = false;
    }
}

pub struct View {
    pub scale: f32,
    pub size: V2,
}
