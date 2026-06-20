use skia_safe::{Font, FontStyle};

use crate::font::font::FontInstance;

mod font;

pub enum Fonts {
    NotoSans(FontInstance),
    Roboto(FontInstance),
}

impl Fonts {
    pub fn noto_sans() -> Self {
        Self::NotoSans(FontInstance::new(
            "Noto Sans CJK JP".into(),
            FontStyle::normal(),
        ))
    }

    pub fn roboto(style: FontStyle) -> Self {
        Self::Roboto(FontInstance::new("Roboto".into(), style))
    }

    pub fn sized(&mut self, size: f32) -> Font {
        match self {
            Fonts::NotoSans(font_instance) => font_instance.from_size(size),
            Fonts::Roboto(font_instance) => font_instance.from_size(size),
        }
    }
}
