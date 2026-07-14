use std::fmt::Display;

use skia_safe::Font;

#[derive(Debug)]
pub struct BoundingBox {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl BoundingBox {
    pub fn from_text(x: f32, y: f32, text: &str, font: &Font) -> Self {
        let mesure = font.measure_str(text, None);
        Self {
            x: x + mesure.1.left,
            y: y + mesure.1.top,
            width: mesure.1.width(),
            height: mesure.1.height(),
        }
    }

    pub fn zero() -> Self {
        Self {
            x: 0.,
            y: 0.,
            width: 0.,
            height: 0.,
        }
    }

    pub fn is_cover(&self, x: f32, y: f32) -> bool {
        self.x <= x && x <= self.x + self.width && self.y <= y && y <= self.y + self.height
    }
}

impl Display for BoundingBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BoundingBox {{ x: {}, y: {}, width: {}, height: {} }}",
            self.x, self.y, self.width, self.height
        )
    }
}
