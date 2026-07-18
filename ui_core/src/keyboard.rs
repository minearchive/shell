pub mod key {
    pub const SPACE: u32 = 0x0020;
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;
    pub const RETURN: u32 = 0xff0d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const HOME: u32 = 0xff50;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
    pub const END: u32 = 0xff57;
    pub const DELETE: u32 = 0xffff;
}

/// The insertable text in a key event's raw UTF-8, or `None` if there is none.
///
/// xkb maps Backspace/Return/Escape/Delete onto C0 control characters, so a raw
/// keysym-to-UTF-8 conversion yields text for keys that are editing commands
/// here. Both backends feed us that conversion, so both must filter it.
///
/// A multiline field wanting `\n` would need to revisit this.
pub fn insertable_text(text: &str) -> Option<String> {
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    (!text.is_empty()).then_some(text)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeyboardEventKind {
    Focus,
    Blur,
    Press { keysym: u32, repeat: bool },
    Release { keysym: u32 },
    /// IME composition, not yet committed.
    Preedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    /// Committed text: both direct typing and IME confirmation arrive here.
    Commit(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyboardEvent {
    pub kind: KeyboardEventKind,
    pub modifiers: Modifiers,
}

impl KeyboardEvent {
    pub fn new(kind: KeyboardEventKind, modifiers: Modifiers) -> Self {
        Self { kind, modifiers }
    }

    pub fn keysym(&self) -> Option<u32> {
        match self.kind {
            KeyboardEventKind::Press { keysym, .. } | KeyboardEventKind::Release { keysym } => {
                Some(keysym)
            }
            _ => None,
        }
    }

    pub fn is_press(&self, keysym: u32) -> bool {
        matches!(self.kind, KeyboardEventKind::Press { keysym: k, .. } if k == keysym)
    }

    pub fn is_release(&self, keysym: u32) -> bool {
        matches!(self.kind, KeyboardEventKind::Release { keysym: k } if k == keysym)
    }

    pub fn commit_text(&self) -> Option<&str> {
        match &self.kind {
            KeyboardEventKind::Commit(text) => Some(text),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertable_text_keeps_printable() {
        assert_eq!(insertable_text("a").as_deref(), Some("a"));
        assert_eq!(insertable_text("日").as_deref(), Some("日"));
    }

    #[test]
    fn insertable_text_rejects_xkb_control_chars() {
        // What xkb hands back for Backspace, Return, Escape and Delete.
        assert_eq!(insertable_text("\u{8}"), None);
        assert_eq!(insertable_text("\r"), None);
        assert_eq!(insertable_text("\u{1b}"), None);
        assert_eq!(insertable_text("\u{7f}"), None);
    }

    #[test]
    fn insertable_text_rejects_empty() {
        assert_eq!(insertable_text(""), None);
    }
}
