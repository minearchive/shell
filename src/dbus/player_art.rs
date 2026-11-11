//! Cover art from the players' `mpris:artUrl`. The players' event stream says nothing when
//! only the art changes, and browsers set it some time after the track, so this polls.

use std::{collections::HashMap, fs, path::Path, process::Command, thread, time::Duration};

use anti_webui::prelude::UiSender as Sender;
use anti_webui::style::ImageSource;
use log::{debug, error};
use mpris::PlayerFinder;
use image::{imageops::FilterType, DynamicImage, RgbaImage};
use ui_core::scheme::color::Color;

use crate::dbus::notification_icon::percent_decode;

const POLL: Duration = Duration::from_secs(1);
/// A player can name any file, even a huge one.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// Larger than the media card, so that art is only ever scaled down when drawn.
const MAX_SIZE: u32 = 512;
const ACCENT_SAMPLES: u32 = 16;
/// How far the accent is mixed towards white, and its partner towards black, for contrast
/// with each other and with the card's scrim.
const ACCENT_LIGHTNESS: f32 = 0.55;
const ON_ACCENT_SHADE: f32 = 0.2;

#[derive(Clone, Debug, PartialEq)]
pub struct Art {
    /// Decoded and shrunk here, off the UI thread.
    pub pixels: ImageSource,
    /// A light color from the art, for what the card emphasizes.
    pub accent: Color,
    pub on_accent: Color,
}

/// Reports `(bus name, art)` whenever a player's art changes.
pub fn start_watcher(sender: Sender<(String, Option<Art>)>) {
    thread::spawn(move || {
        let finder = match PlayerFinder::new() {
            Ok(finder) => finder,
            Err(err) => {
                error!("Failed to connect to D-Bus: {err}");
                return;
            }
        };
        let mut seen: HashMap<String, String> = HashMap::new();
        loop {
            let players: Vec<_> = finder
                .iter_players()
                .map(|players| players.filter_map(Result::ok).collect())
                .unwrap_or_default();
            seen.retain(|bus_name, _| players.iter().any(|p| p.bus_name() == bus_name));

            for player in &players {
                let Ok(metadata) = player.get_metadata() else {
                    continue;
                };
                let url = metadata.art_url().unwrap_or_default();
                let bus_name = player.bus_name();
                if seen.get(bus_name).map(String::as_str) != Some(url) {
                    seen.insert(bus_name.to_string(), url.to_string());
                    let _ = sender.send((bus_name.to_string(), load(url)));
                }
            }
            thread::sleep(POLL);
        }
    });
}

fn load(url: &str) -> Option<Art> {
    let image = image::load_from_memory(&read(url)?)
        .inspect_err(|err| debug!("Could not decode player art {url}: {err}"))
        .ok()?;
    let image = shrink(image);
    let accent = accent(&image)?;
    Some(Art {
        pixels: pixels(image),
        accent,
        on_accent: Color {
            r: accent.r * ON_ACCENT_SHADE,
            g: accent.g * ON_ACCENT_SHADE,
            b: accent.b * ON_ACCENT_SHADE,
            a: 1.0,
        },
    })
}

fn read(url: &str) -> Option<Vec<u8>> {
    if let Some(path) = url.strip_prefix("file://") {
        return read_file(Path::new(&percent_decode(path)));
    }
    if url.starts_with("https://") || url.starts_with("http://") {
        return fetch(url);
    }
    None
}

fn read_file(path: &Path) -> Option<Vec<u8>> {
    let metadata = fs::metadata(path).ok()?;
    (metadata.is_file() && metadata.len() <= MAX_FILE_BYTES)
        .then(|| fs::read(path).ok())
        .flatten()
}

/// ponytail: curl rather than an HTTP client and its TLS stack for one rare GET. Without
/// curl in PATH, players with remote art just show none.
fn fetch(url: &str) -> Option<Vec<u8>> {
    let output = Command::new("curl")
        .args(["-sfL", "--max-time", "5", "--proto", "=http,https"])
        .args(["--max-filesize", &MAX_FILE_BYTES.to_string(), "--", url])
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

/// Caps the art's size, keeping its shape.
fn shrink(image: DynamicImage) -> RgbaImage {
    if image.width().max(image.height()) > MAX_SIZE {
        image.resize(MAX_SIZE, MAX_SIZE, FilterType::Triangle).into_rgba8()
    } else {
        image.into_rgba8()
    }
}

/// The image's pixels as AntiWebUI takes them: RGBA with straight alpha.
pub fn pixels(image: RgbaImage) -> ImageSource {
    let (width, height) = image.dimensions();
    ImageSource::rgba(width, height, image.into_raw())
}

/// The art's most vivid color, lightened. Art with no color in it comes out near white.
fn accent(image: &RgbaImage) -> Option<Color> {
    let small = image::imageops::resize(image, ACCENT_SAMPLES, ACCENT_SAMPLES, FilterType::Triangle);
    let vividness = |[r, g, b]: [f32; 3]| r.max(g).max(b) - r.min(g).min(b);
    let vivid = small
        .pixels()
        .map(|p| [p[0], p[1], p[2]].map(|channel| channel as f32 / 255.0))
        .max_by(|a, b| vividness(*a).total_cmp(&vividness(*b)))?;

    let brightest = vivid[0].max(vivid[1]).max(vivid[2]).max(f32::EPSILON);
    let [r, g, b] = vivid.map(|c| c / brightest * (1.0 - ACCENT_LIGHTNESS) + ACCENT_LIGHTNESS);
    Some(Color { r, g, b, a: 1.0 })
}

#[cfg(test)]
mod tests {
    use image::Rgba;

    use super::*;

    fn solid_with_spot(spot: [u8; 3]) -> RgbaImage {
        RgbaImage::from_fn(64, 64, |x, y| {
            let [r, g, b] = if x < 32 && y < 32 { spot } else { [40; 3] };
            Rgba([r, g, b, 255])
        })
    }

    #[test]
    fn the_accent_follows_the_most_vivid_color_and_stays_light() {
        let accent = accent(&solid_with_spot([200, 0, 0])).unwrap();
        assert!(
            accent.r > accent.g + 0.2 && accent.g == accent.b,
            "{accent:?}"
        );
        assert!(accent.g >= ACCENT_LIGHTNESS - 0.01, "{accent:?}");
    }

    #[test]
    fn large_art_is_capped_and_keeps_its_shape() {
        let image = shrink(DynamicImage::new_rgba8(2000, 1000));
        assert_eq!(image.dimensions(), (512, 256));
        assert_eq!(shrink(DynamicImage::new_rgba8(100, 50)).dimensions(), (100, 50));
    }

    #[test]
    fn only_files_and_the_web_are_fetched() {
        assert_eq!(read(""), None);
        assert_eq!(read("/etc/passwd"), None);
        assert_eq!(read("ftp://example.com/a.png"), None);
        assert_eq!(read("file:///nonexistent/a%20b.png"), None);
    }
}
