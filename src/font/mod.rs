use skia_safe::{Font, FontStyle};

use crate::font::font::FontInstance;

mod font;

pub enum Fonts {
    NotoSans(FontInstance),
}

impl Fonts {
    pub fn noto_sans() -> Self {
        Self::NotoSans(FontInstance::new(
            "Noto Sans CJK JP".into(),
            FontStyle::normal(),
        ))
    }

    pub fn sized(&mut self, size: f32) -> Font {
        match self {
            Fonts::NotoSans(font_instance) => font_instance.from_size(size),
        }
    }
}
