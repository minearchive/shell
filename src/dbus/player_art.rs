//! Cover art from the players' `mpris:artUrl`. The players' event stream says nothing when
//! only the art changes, and browsers set it some time after the track, so this polls.

use std::{collections::HashMap, fs, path::Path, process::Command, thread, time::Duration};

use anti_webui::prelude::UiSender as Sender;
use anti_webui::style::ImageSource;
use log::{debug, error};
use mpris::PlayerFinder;
use skia_safe::{
    image::CachingHint, surfaces, AlphaType, ColorType, Data, FilterMode, Image, ImageInfo,
    MipmapMode, Paint, Rect, SamplingOptions,
};
use ui_core::scheme::color::Color;

use crate::dbus::notification_icon::percent_decode;

const POLL: Duration = Duration::from_secs(1);
/// A player can name any file, even a huge one.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// Larger than the media card, so that art is only ever scaled down when drawn.
const MAX_SIZE: i32 = 512;
const ACCENT_SAMPLES: i32 = 16;
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
    let image = Image::from_encoded(Data::new_copy(&read(url)?));
    if image.is_none() {
        debug!("Could not decode player art {url}");
    }
    let image = shrink(&image?)?;
    let accent = accent(&image)?;
    Some(Art {
        pixels: pixels(&image)?,
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

/// Decodes the image, so repaints don't, and caps its size.
fn shrink(image: &Image) -> Option<Image> {
    let scale = (MAX_SIZE as f32 / image.width().max(image.height()) as f32).min(1.0);
    let width = (image.width() as f32 * scale).round().max(1.0);
    let height = (image.height() as f32 * scale).round().max(1.0);
    resized(image, width as i32, height as i32)
}

fn resized(image: &Image, width: i32, height: i32) -> Option<Image> {
    let mut surface = surfaces::raster_n32_premul((width, height))?;
    let rect = Rect::from_iwh(width, height);
    let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear);
    surface.canvas().draw_image_rect_with_sampling_options(
        image,
        None,
        rect,
        sampling,
        &Paint::default(),
    );
    Some(surface.image_snapshot())
}

/// The image's pixels as AntiWebUI takes them: RGBA with straight alpha.
pub fn pixels(image: &Image) -> Option<ImageSource> {
    let (width, height) = (image.width(), image.height());
    let info = ImageInfo::new((width, height), ColorType::RGBA8888, AlphaType::Unpremul, None);
    let mut data = vec![0u8; width as usize * height as usize * 4];
    image
        .read_pixels(&info, &mut data, width as usize * 4, (0, 0), CachingHint::Disallow)
        .then(|| ImageSource::rgba(width as u32, height as u32, data))
}

/// The art's most vivid color, lightened. Art with no color in it comes out near white.
fn accent(image: &Image) -> Option<Color> {
    let mut small = surfaces::raster_n32_premul((ACCENT_SAMPLES, ACCENT_SAMPLES))?;
    let rect = Rect::from_iwh(ACCENT_SAMPLES, ACCENT_SAMPLES);
    let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::None);
    small.canvas().draw_image_rect_with_sampling_options(
        image,
        None,
        rect,
        sampling,
        &Paint::default(),
    );
    let pixels = small.peek_pixels()?;
    let vividness = |[r, g, b]: [f32; 3]| r.max(g).max(b) - r.min(g).min(b);
    let vivid = (0..ACCENT_SAMPLES * ACCENT_SAMPLES)
        .map(|i| pixels.get_color((i % ACCENT_SAMPLES, i / ACCENT_SAMPLES)))
        .map(|c| [c.r(), c.g(), c.b()].map(|channel| channel as f32 / 255.0))
        .max_by(|a, b| vividness(*a).total_cmp(&vividness(*b)))?;

    let brightest = vivid[0].max(vivid[1]).max(vivid[2]).max(f32::EPSILON);
    let [r, g, b] = vivid.map(|c| c / brightest * (1.0 - ACCENT_LIGHTNESS) + ACCENT_LIGHTNESS);
    Some(Color { r, g, b, a: 1.0 })
}

#[cfg(test)]
mod tests {
    use skia_safe::Color as SkColor;

    use super::*;

    fn solid_with_spot(spot: SkColor) -> Image {
        let mut surface = surfaces::raster_n32_premul((64, 64)).unwrap();
        surface.canvas().clear(SkColor::from_rgb(40, 40, 40));
        surface.canvas().draw_rect(
            Rect::from_xywh(0.0, 0.0, 32.0, 32.0),
            &Paint::new(skia_safe::Color4f::from(spot), None),
        );
        surface.image_snapshot()
    }

    #[test]
    fn the_accent_follows_the_most_vivid_color_and_stays_light() {
        let accent = accent(&solid_with_spot(SkColor::from_rgb(200, 0, 0))).unwrap();
        assert!(
            accent.r > accent.g + 0.2 && accent.g == accent.b,
            "{accent:?}"
        );
        assert!(accent.g >= ACCENT_LIGHTNESS - 0.01, "{accent:?}");
    }

    #[test]
    fn large_art_is_capped_and_keeps_its_shape() {
        let mut surface = surfaces::raster_n32_premul((2000, 1000)).unwrap();
        surface.canvas().clear(SkColor::RED);
        let image = shrink(&surface.image_snapshot()).unwrap();
        assert_eq!((image.width(), image.height()), (512, 256));
    }

    #[test]
    fn only_files_and_the_web_are_fetched() {
        assert_eq!(read(""), None);
        assert_eq!(read("/etc/passwd"), None);
        assert_eq!(read("ftp://example.com/a.png"), None);
        assert_eq!(read("file:///nonexistent/a%20b.png"), None);
    }
}
