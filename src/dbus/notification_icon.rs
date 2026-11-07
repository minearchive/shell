//! Turns what a `Notify` call says about its icon into an image for the island. Runs on
//! the D-Bus thread, so the bar never waits on a disk or a decode.

use std::{collections::HashMap, env, fs, path::PathBuf};

use log::debug;
use skia_safe::{
    images, surfaces, AlphaType, ColorType, Data, FilterMode, Image, ImageInfo, MipmapMode, Paint,
    RRect, Rect, SamplingOptions,
};
use zbus::zvariant::Value;

/// The size icons are drawn at in the island, which they get scaled to right here so
/// that repaints don't have to.
pub const ICON_SIZE: i32 = 48;
const ICON_RADIUS: f32 = 10.0;
/// More than any real icon. A sender can name any file, even a huge one.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DIMENSION: i32 = 4096;
const HICOLOR_SIZES: [&str; 7] = [
    "96x96", "128x128", "64x64", "256x256", "48x48", "512x512", "32x32",
];

/// The `(iiibiiay)` of the spec's image hints: width, height, rowstride, has_alpha,
/// bits_per_sample, channels, data.
type RawImage = (i32, i32, i32, bool, i32, i32, Vec<u8>);

/// Tries each source in the spec's order, moving on from any that doesn't work out.
/// `desktop-entry` isn't an icon by the spec, but usually names one.
pub fn resolve(app_icon: &str, hints: &HashMap<String, Value<'_>>) -> Option<Image> {
    let hint_str = |key: &str| hints.get(key).and_then(|v| <&str>::try_from(v).ok());
    let raw = |key: &str| hints.get(key).and_then(from_hint);

    raw("image-data")
        .or_else(|| raw("image_data"))
        .or_else(|| from_name(hint_str("image-path")?))
        .or_else(|| from_name(hint_str("image_path")?))
        .or_else(|| from_name(app_icon))
        .or_else(|| raw("icon_data"))
        .or_else(|| from_name(hint_str("desktop-entry")?))
        .and_then(|image| thumbnail(&image))
}

fn from_hint(value: &Value<'_>) -> Option<Image> {
    let raw: RawImage = value.try_clone().ok()?.try_into().ok()?;
    from_raw(raw)
}

fn from_raw((width, height, stride, _alpha, bits, channels, data): RawImage) -> Option<Image> {
    let dimensions_ok =
        (1..=MAX_DIMENSION).contains(&width) && (1..=MAX_DIMENSION).contains(&height);
    if !dimensions_ok || bits != 8 || !matches!(channels, 3 | 4) {
        return None;
    }
    let (w, h, channels) = (width as usize, height as usize, channels as usize);
    let stride = usize::try_from(stride)
        .ok()
        .filter(|s| *s >= w * channels)?;
    // The last row needn't carry its padding.
    if data.len() < stride * (h - 1) + w * channels {
        return None;
    }

    let mut rgba = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        let start = row * stride;
        for pixel in data[start..start + w * channels].chunks_exact(channels) {
            rgba.extend_from_slice(&pixel[..3]);
            rgba.push(if channels == 4 { pixel[3] } else { 255 });
        }
    }
    let info = ImageInfo::new(
        (width, height),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    images::raster_from_data(&info, Data::new_copy(&rgba), w * 4)
}

fn from_name(name: &str) -> Option<Image> {
    let path = icon_path(name, &data_dirs())?;
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        debug!("Skipping notification icon {path:?}");
        return None;
    }
    let bytes = fs::read(&path).ok()?;
    let image = Image::from_encoded(Data::new_copy(&bytes));
    if image.is_none() {
        debug!("Could not decode notification icon {path:?}");
    }
    image
}

/// A `file://` URI, an absolute path, or an icon's name.
fn icon_path(name: &str, data_dirs: &[PathBuf]) -> Option<PathBuf> {
    if let Some(path) = name.strip_prefix("file://") {
        return Some(percent_decode(path).into());
    }
    if name.starts_with('/') {
        return Some(name.into());
    }
    if name.is_empty() || name.contains('/') {
        return None;
    }
    find_in_theme(name, data_dirs)
}

/// ponytail: PNGs in hicolor and pixmaps only, which is what apps install for
/// themselves. Named icons from the user's theme are mostly SVGs, which take Skia's `svg`
/// feature or resvg to decode.
fn find_in_theme(name: &str, data_dirs: &[PathBuf]) -> Option<PathBuf> {
    let file = format!("{name}.png");
    let in_hicolor = HICOLOR_SIZES.iter().flat_map(|size| {
        data_dirs
            .iter()
            .map(move |dir| dir.join("icons/hicolor").join(size).join("apps"))
    });
    let in_pixmaps = data_dirs.iter().map(|dir| dir.join("pixmaps"));
    in_hicolor
        .chain(in_pixmaps)
        .map(|dir| dir.join(&file))
        .find(|path| path.is_file())
}

fn data_dirs() -> Vec<PathBuf> {
    let home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let dirs = env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    home.into_iter()
        .chain(dirs.split(':').map(PathBuf::from))
        .collect()
}

/// A sequence that isn't valid hex stays as it is.
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%')
            .then(|| text.get(i + 1..i + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match hex {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Fits `image` in an icon-sized square, centered, with rounded corners.
fn thumbnail(image: &Image) -> Option<Image> {
    let mut surface = surfaces::raster_n32_premul((ICON_SIZE, ICON_SIZE))?;
    let canvas = surface.canvas();
    let size = ICON_SIZE as f32;
    let scale = size / image.width().max(image.height()) as f32;
    let (width, height) = (image.width() as f32 * scale, image.height() as f32 * scale);
    let rect = Rect::from_xywh((size - width) / 2.0, (size - height) / 2.0, width, height);

    canvas.clip_rrect(
        RRect::new_rect_xy(rect, ICON_RADIUS, ICON_RADIUS),
        None,
        true,
    );
    let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear);
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    canvas.draw_image_rect_with_sampling_options(image, None, rect, sampling, &paint);
    Some(surface.image_snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;

    use zbus::zvariant::Structure;

    fn raw(width: i32, height: i32, stride: i32, channels: i32, data: Vec<u8>) -> RawImage {
        (width, height, stride, channels == 4, 8, channels, data)
    }

    fn color_at(image: &Image, x: i32, y: i32) -> skia_safe::Color {
        let mut surface = surfaces::raster_n32_premul((ICON_SIZE, ICON_SIZE)).unwrap();
        surface.canvas().draw_image(image, (0, 0), None);
        surface.peek_pixels().unwrap().get_color((x, y))
    }

    #[test]
    fn raw_images_honor_rowstride_and_missing_alpha() {
        // Two rows of one RGB pixel, with a padding byte that must not shift the second.
        let data = vec![1, 2, 3, 99, 4, 5, 6];
        let image = from_raw(raw(1, 2, 4, 3, data)).unwrap();
        assert_eq!((image.width(), image.height()), (1, 2));
        assert_eq!(image.alpha_type(), AlphaType::Unpremul);

        let thumb = thumbnail(&from_raw(raw(1, 1, 3, 3, vec![255, 0, 0])).unwrap()).unwrap();
        assert_eq!(
            color_at(&thumb, 24, 24),
            skia_safe::Color::from_rgb(255, 0, 0)
        );
    }

    #[test]
    fn malformed_raw_images_are_rejected() {
        for bad in [
            raw(0, 1, 3, 3, vec![0; 3]),
            raw(1, -1, 3, 3, vec![0; 3]),
            raw(MAX_DIMENSION + 1, 1, (MAX_DIMENSION + 1) * 3, 3, vec![0; 3]),
            raw(1, 1, 3, 2, vec![0; 3]),
            raw(2, 1, 3, 3, vec![0; 6]),
            raw(1, 2, 3, 3, vec![0; 5]),
            raw(1, 2, -3, 3, vec![0; 6]),
        ] {
            assert!(from_raw(bad.clone()).is_none(), "{:?}", &bad.0);
        }
        let wrong_bits = (1, 1, 3, false, 16, 3, vec![0; 3]);
        assert!(from_raw(wrong_bits).is_none());
    }

    #[test]
    fn thumbnails_fit_inside_without_stretching() {
        let wide = from_raw(raw(4, 1, 12, 3, [0, 255, 0].repeat(4))).unwrap();
        let thumb = thumbnail(&wide).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (ICON_SIZE, ICON_SIZE));
        assert_eq!(color_at(&thumb, 24, 24).g(), 255);
        assert_eq!(color_at(&thumb, 24, 2).a(), 0, "letterboxed");
    }

    #[test]
    fn image_hints_arrive_as_dbus_structures() {
        let value = Value::from(Structure::from((
            1i32,
            1i32,
            3i32,
            false,
            8i32,
            3i32,
            vec![1u8, 2, 3],
        )));
        let mut hints = HashMap::new();
        hints.insert("image-data".to_string(), value);
        assert!(resolve("", &hints).is_some());
        assert!(resolve("", &HashMap::new()).is_none());
    }

    #[test]
    fn names_resolve_to_paths() {
        let root = env::temp_dir().join(format!("notification_icon_{}", std::process::id()));
        let dirs = [root.clone()];
        let small = root.join("icons/hicolor/48x48/apps");
        let large = root.join("icons/hicolor/96x96/apps");
        let pixmaps = root.join("pixmaps");
        for dir in [&small, &large, &pixmaps] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(small.join("both.png"), "").unwrap();
        fs::write(large.join("both.png"), "").unwrap();
        fs::write(small.join("small.png"), "").unwrap();
        fs::write(pixmaps.join("old.png"), "").unwrap();

        let path = |name| icon_path(name, &dirs);
        assert_eq!(path("both"), Some(large.join("both.png")));
        assert_eq!(path("small"), Some(small.join("small.png")));
        assert_eq!(path("old"), Some(pixmaps.join("old.png")));
        assert_eq!(path("missing"), None);
        assert_eq!(path(""), None);
        assert_eq!(path("../../old"), None);
        assert_eq!(path("/a/b.png"), Some("/a/b.png".into()));
        assert_eq!(path("file:///a%20b/c.png"), Some("/a b/c.png".into()));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn percent_decoding_leaves_invalid_sequences_alone() {
        assert_eq!(percent_decode("a%20b%E3%81%82"), "a bあ");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%2"), "%zz%2");
    }
}
