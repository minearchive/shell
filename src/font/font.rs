use skia_bindings::SkTypeface;
use skia_safe::{Font, FontMgr, FontStyle, RCHandle};

pub struct FontInstance {
    pub(crate) _mgr: FontMgr,
    pub(crate) _name: String,
    pub typeface: RCHandle<SkTypeface>,
}

impl FontInstance {
    pub fn new(name: String, style: FontStyle) -> Self {
        let mgr = FontMgr::new();

        let typeface = mgr
            .legacy_make_typeface(name.as_str(), style)
            .expect("Failed to create typeface");

        Self {
            _mgr: mgr,
            _name: name,
            typeface,
        }
    }

    pub fn from_size(&self, size: f32) -> Font {
        Font::new(self.typeface.clone(), size)
    }
}
