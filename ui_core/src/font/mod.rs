use std::cell::RefCell;
use std::collections::HashMap;

use skia_safe::{Canvas, Font, FontMgr, FontStyle, Paint, Point, Typeface};

struct Entry {
    typeface: Typeface,
    style: FontStyle,
    /// char -> a typeface that can draw it, caching `match_family_style_character`.
    /// `None` means no font on the system covers the char (drawn as tofu).
    fallback: RefCell<HashMap<char, Option<Typeface>>>,
}

pub struct FontBook {
    mgr: FontMgr,
    typefaces: HashMap<String, Entry>,
}

impl FontBook {
    pub fn new() -> Self {
        Self {
            mgr: FontMgr::new(),
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
            .mgr
            .legacy_make_typeface(family, style)
            .unwrap_or_else(|| panic!("Failed to load font: {family}"));
        self.typefaces.insert(
            key.into(),
            Entry {
                typeface,
                style,
                fallback: RefCell::new(HashMap::new()),
            },
        );
        self
    }

    pub fn sized(&self, key: &str, size: f32) -> Font {
        Font::new(self.entry(key).typeface.clone(), size)
    }

    /// Splits `text` into stretches drawable by a single typeface. Chars the
    /// registered font has no glyph for are filled in from system fonts.
    ///
    /// ponytail: no shaping (splits per char). ZWJ emoji, combining marks and
    /// Arabic will break. Move to skia-safe's textlayout feature (skparagraph)
    /// if that matters.
    pub fn runs(&self, key: &str, size: f32, text: &str) -> Vec<(String, Font)> {
        let entry = self.entry(key);
        let mut runs: Vec<(String, Font)> = Vec::new();
        for c in text.chars() {
            let typeface = if entry.typeface.unichar_to_glyph(c as i32) != 0 {
                entry.typeface.clone()
            } else {
                entry
                    .fallback
                    .borrow_mut()
                    .entry(c)
                    .or_insert_with(|| {
                        self.mgr
                            .match_family_style_character("", entry.style, &[], c as i32)
                    })
                    .clone()
                    .unwrap_or_else(|| entry.typeface.clone())
            };
            match runs.last_mut() {
                Some((run, font)) if font.typeface().unique_id() == typeface.unique_id() => {
                    run.push(c)
                }
                _ => runs.push((c.to_string(), Font::new(typeface, size))),
            }
        }
        runs
    }

    /// Advance width including fallback runs.
    pub fn measure(&self, key: &str, size: f32, text: &str) -> f32 {
        self.runs(key, size, text)
            .iter()
            .map(|(run, font)| font.measure_str(run, None).0)
            .sum()
    }

    /// Draws from `at` (baseline, left edge) with fallback. Returns the advance width.
    pub fn draw(
        &self,
        canvas: &Canvas,
        key: &str,
        size: f32,
        text: &str,
        at: Point,
        paint: &Paint,
    ) -> f32 {
        let mut x = at.x;
        for (run, font) in self.runs(key, size, text) {
            canvas.draw_str(&run, Point::new(x, at.y), &font, paint);
            x += font.measure_str(&run, Some(paint)).0;
        }
        x - at.x
    }

    fn entry(&self, key: &str) -> &Entry {
        self.typefaces
            .get(key)
            .unwrap_or_else(|| panic!("Font key not registered: {key}"))
    }
}

impl Default for FontBook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> FontBook {
        let mut book = FontBook::new();
        book.register("t", "DejaVu Sans", FontStyle::normal());
        book
    }

    #[test]
    fn runs_split_on_missing_glyphs() {
        let book = book();
        // DejaVu Sans has no CJK, so "abc" and "日本語" land in separate runs.
        let runs = book.runs("t", 16.0, "abc日本語abc");
        let texts: Vec<&str> = runs.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(texts, vec!["abc", "日本語", "abc"], "runs: {texts:?}");
        assert!(runs[0].1.typeface().unique_id() != runs[1].1.typeface().unique_id());

        // Pure ASCII stays a single run, and measures the same as a plain Font.
        let font = book.sized("t", 16.0);
        assert_eq!(book.runs("t", 16.0, "abc").len(), 1);
        assert_eq!(
            book.measure("t", 16.0, "abc"),
            font.measure_str("abc", None).0
        );

        // CJK measures non-zero once fallback kicks in.
        assert!(book.measure("t", 16.0, "日本語") > 0.0);
    }
}
