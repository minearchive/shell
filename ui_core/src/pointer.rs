pub type Point = (f64, f64);

pub mod button {
    pub const LEFT: u32 = 0x110; // 272
    pub const RIGHT: u32 = 0x111; // 273
    pub const MIDDLE: u32 = 0x112; // 274
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AxisScroll {
    pub absolute: f64,
    pub discrete: i32,
    pub stop: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerEventKind {
    Enter,
    Leave,
    Motion,
    Press {
        button: u32,
    },
    Release {
        button: u32,
    },
    Axis {
        horizontal: AxisScroll,
        vertical: AxisScroll,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    pub position: Point,
    pub kind: PointerEventKind,
}

impl PointerEvent {
    pub fn new(position: Point, kind: PointerEventKind) -> Self {
        Self { position, kind }
    }

    pub fn x(&self) -> f64 {
        self.position.0
    }

    pub fn y(&self) -> f64 {
        self.position.1
    }

    pub fn button(&self) -> Option<u32> {
        match self.kind {
            PointerEventKind::Press { button } | PointerEventKind::Release { button } => {
                Some(button)
            }
            _ => None,
        }
    }

    pub fn is_press(&self, button: u32) -> bool {
        matches!(self.kind, PointerEventKind::Press { button: b } if b == button)
    }

    pub fn is_release(&self, button: u32) -> bool {
        matches!(self.kind, PointerEventKind::Release { button: b } if b == button)
    }
}
