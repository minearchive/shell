use skia_bindings::SkTypeface;
use skia_safe::{Font, FontMgr, FontStyle, RCHandle};

pub struct FontInstance {
    pub(crate) font_mgr: FontMgr,
    pub(crate) name: String,
    pub typeface: RCHandle<SkTypeface>,
}

impl FontInstance {
    pub fn new(name: String, style: FontStyle) -> Self {
        let mgr = FontMgr::new();

        let typeface = mgr
            .legacy_make_typeface(name.as_str(), style)
            .expect("Failed to create typeface");

        Self {
            font_mgr: mgr,
            name,
            typeface,
        }
    }

    pub fn from_size(&mut self, size: f32) -> Font {
        Font::new(self.typeface.clone(), size)
    }
}
