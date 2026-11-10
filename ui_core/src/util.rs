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

/// The first line of `text`, cut off with an ellipsis at `max_width` wherever that falls.
pub fn elide(text: &str, max_width: f32, measure: impl Fn(&str) -> f32) -> String {
    let line = text.lines().next().unwrap_or_default();
    // A notification's body can be thousands of chars, and measuring is slow with as many, so
    // this measures short prefixes: doubling until one is too wide, then halving between.
    let ends: Vec<usize> = line.char_indices().map(|(i, _)| i).chain([line.len()]).collect();
    let chars = ends.len() - 1;
    let mut too_long = 16;
    while too_long < chars && measure(&line[..ends[too_long]]) <= max_width {
        too_long *= 2;
    }
    if too_long >= chars {
        if measure(line) <= max_width {
            return line.to_string();
        }
        too_long = chars;
    }
    let with_ellipsis = |chars: usize| format!("{}…", &line[..ends[chars]]);
    let mut fits = 0;
    while too_long - fits > 1 {
        let mid = (fits + too_long) / 2;
        if measure(&with_ellipsis(mid)) <= max_width {
            fits = mid;
        } else {
            too_long = mid;
        }
    }
    with_ellipsis(fits)
}

/// Breaks `text` into lines no wider than `max_width`, preferring spaces and
/// otherwise breaking between chars (CJK has no spaces). Blank lines are dropped.
/// Past `max_lines`, the last line ends in an ellipsis. A single char wider than `max_width` keeps its
/// own line rather than being lost.
pub fn wrap_text(
    text: &str,
    max_width: f32,
    max_lines: usize,
    measure: impl Fn(&str) -> f32,
) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        if lines.len() > max_lines {
            break;
        }
        let mut line = String::new();
        for c in paragraph.chars() {
            line.push(c);
            if measure(&line) <= max_width || line.chars().count() == 1 {
                continue;
            }
            line.pop();
            let rest = match line.rfind(' ') {
                Some(space) if space > 0 && c != ' ' => {
                    let rest = line[space + 1..].to_string();
                    line.truncate(space);
                    rest
                }
                _ => String::new(),
            };
            lines.push(std::mem::replace(&mut line, rest));
            if c != ' ' {
                line.push(c);
            }
            if lines.len() > max_lines {
                break;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }

    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            while !last.is_empty() && measure(&format!("{last}…")) > max_width {
                last.pop();
            }
            last.push('…');
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elide_cuts_at_the_width_rather_than_at_a_space() {
        let measure = |s: &str| s.chars().count() as f32;
        assert_eq!(elide("short", 10.0, measure), "short");
        assert_eq!(elide("one two three", 10.0, measure), "one two t…");
        assert_eq!(elide("first\nsecond", 10.0, measure), "first");
        assert_eq!(elide("", 10.0, measure), "");
        assert_eq!(elide("toolong", 0.5, measure), "…", "nothing fits but the ellipsis");
    }

    #[test]
    fn elide_measures_a_long_line_only_a_few_times() {
        let calls = std::cell::Cell::new(0);
        let measure = |s: &str| {
            calls.set(calls.get() + 1);
            s.chars().count() as f32
        };
        let long = "長".repeat(5000);
        assert_eq!(elide(&long, 10.0, measure), format!("{}…", "長".repeat(9)));
        assert!(calls.get() < 20, "measured {} times", calls.get());
    }

    // Every char is 1 wide.
    fn wrap(text: &str, max_width: f32, max_lines: usize) -> Vec<String> {
        wrap_text(text, max_width, max_lines, |s| s.chars().count() as f32)
    }

    #[test]
    fn prefers_spaces_and_breaks_cjk_between_chars() {
        assert_eq!(wrap("hello big world", 9.0, 5), ["hello big", "world"]);
        assert_eq!(wrap("日本語の通知", 4.0, 5), ["日本語の", "通知"]);
        assert_eq!(wrap("first\nsecond", 9.0, 5), ["first", "second"]);
    }

    #[test]
    fn ellipsizes_only_when_text_was_dropped() {
        assert_eq!(wrap("aaaa bbbb cccc", 4.0, 2), ["aaaa", "bbb…"]);
        assert_eq!(wrap("one\ntwo\nthree", 5.0, 2), ["one", "two…"]);
        assert_eq!(wrap("aaaa bbbb", 4.0, 2), ["aaaa", "bbbb"]);
    }

    #[test]
    fn handles_empty_and_oversized_input() {
        assert!(wrap("", 4.0, 2).is_empty());
        assert_eq!(wrap("ab", 0.5, 5), ["a", "b"]);
    }
}
