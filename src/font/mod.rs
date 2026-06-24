use std::collections::HashMap;

use skia_bindings::SkTypeface;
use skia_safe::{Font, FontMgr, FontStyle, RCHandle};

pub struct FontBook {
    _mgr: FontMgr,
    typefaces: HashMap<String, RCHandle<SkTypeface>>,
}

impl FontBook {
    pub fn new() -> Self {
        Self {
            _mgr: FontMgr::new(),
            typefaces: HashMap::new(),
        }
    }

    pub fn register(
        &mut self,
        key: impl Into<String>,
        family: &str,
        style: FontStyle,
    ) -> &mut Self {
        let typeface = self
            ._mgr
            .legacy_make_typeface(family, style)
            .unwrap_or_else(|| panic!("Failed to load font: {family}"));
        self.typefaces.insert(key.into(), typeface);
        self
    }

    pub fn sized(&self, key: &str, size: f32) -> Font {
        let typeface = self
            .typefaces
            .get(key)
            .unwrap_or_else(|| panic!("Font key not registered: {key}"));
        Font::new(typeface.clone(), size)
    }
}
