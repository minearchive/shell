//! What the toolbox shows. To add an entry, push one to [`default_items`]: most are
//! an [`InfoItem`] or a [`SliderItem`], and anything else implements [`ToolboxItem`].

use m3_widget::{icon::BatteryMark, Icon, Segment, SegmentedButton, Slider, Widget};
use skia_safe::{Canvas, Color4f, Paint, Point, RRect, Rect};
use tokio::sync::mpsc::UnboundedSender;

use ui_core::{
    font::FontBook, geometry::LayoutRect, pointer::PointerEvent, scheme::ColorTheme,
    util::wrap_text,
};

use crate::{
    system::{BatteryState, Network, PowerProfile, SystemCommand, SystemStatus},
    ui::UIState,
};

const FONT: &str = "noto_sans";
const ICON_SIZE: f32 = 24.0;
const INFO_HEIGHT: f32 = 56.0;
const INFO_RADIUS: f32 = 16.0;
const INFO_PADDING: f32 = 12.0;
const TITLE_SIZE: f32 = 14.0;
const DETAIL_SIZE: f32 = 12.0;
const SLIDER_HEIGHT: f32 = 44.0;
const SLIDER_GAP: f32 = 12.0;
const VALUE_WIDTH: f32 = 40.0;
const CHOICE_HEIGHT: f32 = 40.0;
/// On top of the panel's gap: the outlined buttons fill their row, unlike the thin sliders.
const CHOICE_TOP_MARGIN: f32 = 6.0;
const CAPTION_HEIGHT: f32 = 16.0;
/// Keeps a dragged-down backlight from turning the screen fully black.
const MIN_BRIGHTNESS: f32 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailWidth {
    /// A small line across the top of the panel, above everything else.
    Caption,
    /// Half the panel. Half-width entries pair up in rows at the top of the panel.
    Half,
    /// The panel's full width, stacked below the half-width rows.
    Full,
}

pub trait ToolboxItem {
    /// The icon for the row in the bar, which flies to [`Self::icon_box`] as the panel
    /// opens, so `draw` leaves it out. `None` keeps this entry to the panel.
    fn bar_icon(&self, status: &SystemStatus) -> Option<Icon>;
    /// Where this entry's icon sits when the entry is at `rect`.
    fn icon_box(&self, rect: LayoutRect) -> Rect;
    /// Whether the bar's icon fades out on its way to [`Self::icon_box`], for an entry
    /// that shows no icon of its own in the panel.
    fn icon_fades(&self) -> bool {
        false
    }
    fn width(&self) -> DetailWidth;
    /// Zero leaves the entry out of the panel, gap included.
    fn height(&self, status: &SystemStatus) -> f32;
    /// Draws the entry's detail in `rect`. Returns whether it is still animating.
    fn draw(
        &mut self,
        canvas: &Canvas,
        rect: LayoutRect,
        state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> bool;
    /// A pointer event anywhere in the panel. Returns whether it needs a repaint.
    fn on_pointer(&mut self, _event: &PointerEvent) -> bool {
        false
    }
}

/// A read-only entry: an icon, a title and one line of detail.
pub struct InfoItem {
    width: DetailWidth,
    title: &'static str,
    read: fn(&SystemStatus) -> (Icon, String),
}

impl InfoItem {
    pub fn new(
        width: DetailWidth,
        title: &'static str,
        read: fn(&SystemStatus) -> (Icon, String),
    ) -> Self {
        Self { width, title, read }
    }
}

impl ToolboxItem for InfoItem {
    fn bar_icon(&self, status: &SystemStatus) -> Option<Icon> {
        Some((self.read)(status).0)
    }

    fn icon_box(&self, rect: LayoutRect) -> Rect {
        tile_icon_box(rect)
    }

    fn width(&self) -> DetailWidth {
        self.width
    }

    fn height(&self, _status: &SystemStatus) -> f32 {
        INFO_HEIGHT
    }

    fn draw(
        &mut self,
        canvas: &Canvas,
        rect: LayoutRect,
        state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> bool {
        let (_, detail) = (self.read)(&state.system);
        let text_right = rect.x + rect.width - INFO_PADDING;
        draw_tile(canvas, rect, text_right, self.title, &detail, fonts, theme);
        false
    }
}

fn tile_icon_box(rect: LayoutRect) -> Rect {
    let top = rect.y + (rect.height - ICON_SIZE) / 2.0;
    Rect::from_xywh(rect.x + INFO_PADDING, top, ICON_SIZE, ICON_SIZE)
}

/// A tile's background, title and one line of detail, leaving room for its icon.
fn draw_tile(
    canvas: &Canvas,
    rect: LayoutRect,
    text_right: f32,
    title: &str,
    detail: &str,
    fonts: &FontBook,
    theme: &ColorTheme,
) {
    let mut paint = Paint::new(Color4f::from(theme.surface_container_high), None);
    paint.set_anti_alias(true);
    canvas.draw_rrect(
        RRect::new_rect_xy(rect.to_skia(), INFO_RADIUS, INFO_RADIUS),
        &paint,
    );

    let text_x = tile_icon_box(rect).right + INFO_PADDING;
    let title_baseline = rect.y + INFO_HEIGHT / 2.0 - 3.0;
    paint.set_color4f(Color4f::from(theme.on_surface), None);
    let at = Point::new(text_x, title_baseline);
    fonts.draw(canvas, FONT, TITLE_SIZE, title, at, &paint);

    paint.set_color4f(Color4f::from(theme.on_surface_variant), None);
    let lines = wrap_text(detail, text_right - text_x, 1, |s| {
        fonts.measure(FONT, DETAIL_SIZE, s)
    });
    if let Some(line) = lines.first() {
        let at = Point::new(text_x, title_baseline + DETAIL_SIZE + 5.0);
        fonts.draw(canvas, FONT, DETAIL_SIZE, line, at, &paint);
    }
}

/// A small right-aligned line of text across the top of the panel, which the bar's icon
/// turns into. Takes no space while `read` returns `None`.
pub struct CaptionItem {
    icon: fn(&SystemStatus) -> Icon,
    read: fn(&SystemStatus) -> Option<String>,
}

impl CaptionItem {
    pub fn new(icon: fn(&SystemStatus) -> Icon, read: fn(&SystemStatus) -> Option<String>) -> Self {
        Self { icon, read }
    }
}

impl ToolboxItem for CaptionItem {
    fn bar_icon(&self, status: &SystemStatus) -> Option<Icon> {
        Some((self.icon)(status))
    }

    fn icon_box(&self, rect: LayoutRect) -> Rect {
        Rect::from_xywh(
            rect.x + rect.width - ICON_SIZE,
            rect.y + (rect.height - ICON_SIZE) / 2.0,
            ICON_SIZE,
            ICON_SIZE,
        )
    }

    fn icon_fades(&self) -> bool {
        true
    }

    fn width(&self) -> DetailWidth {
        DetailWidth::Caption
    }

    fn height(&self, status: &SystemStatus) -> f32 {
        if (self.read)(status).is_some() {
            CAPTION_HEIGHT
        } else {
            0.0
        }
    }

    fn draw(
        &mut self,
        canvas: &Canvas,
        rect: LayoutRect,
        state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> bool {
        if let Some(text) = (self.read)(&state.system) {
            let mut paint = Paint::new(Color4f::from(theme.on_surface_variant), None);
            paint.set_anti_alias(true);
            let x = rect.x + rect.width - fonts.measure(FONT, DETAIL_SIZE, &text);
            let at = Point::new(x, rect.y + (rect.height + DETAIL_SIZE) / 2.0 - 1.0);
            fonts.draw(canvas, FONT, DETAIL_SIZE, &text, at, &paint);
        }
        false
    }
}

/// A full-width entry the user adjusts: an icon, an M3 slider and the percentage.
/// Disabled while its value reads as `None`.
pub struct SliderItem {
    in_bar: bool,
    icon: fn(&SystemStatus) -> Icon,
    read: fn(&SystemStatus) -> Option<f32>,
    slider: Slider,
}

impl SliderItem {
    pub fn new(
        min: f32,
        icon: fn(&SystemStatus) -> Icon,
        read: fn(&SystemStatus) -> Option<f32>,
        on_change: impl FnMut(f32) + 'static,
    ) -> Self {
        Self {
            in_bar: true,
            icon,
            read,
            slider: Slider::new(min, 1.0, min).on_change(on_change),
        }
    }

    /// Leaves this entry's icon out of the row in the bar.
    pub fn panel_only(mut self) -> Self {
        self.in_bar = false;
        self
    }
}

impl ToolboxItem for SliderItem {
    fn bar_icon(&self, status: &SystemStatus) -> Option<Icon> {
        self.in_bar.then(|| (self.icon)(status))
    }

    fn icon_box(&self, rect: LayoutRect) -> Rect {
        let top = rect.y + (rect.height - ICON_SIZE) / 2.0;
        Rect::from_xywh(rect.x, top, ICON_SIZE, ICON_SIZE)
    }

    fn width(&self) -> DetailWidth {
        DetailWidth::Full
    }

    fn height(&self, _status: &SystemStatus) -> f32 {
        SLIDER_HEIGHT
    }

    fn draw(
        &mut self,
        canvas: &Canvas,
        rect: LayoutRect,
        state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> bool {
        let status = &state.system;
        let value = (self.read)(status);
        self.slider.set_enabled(value.is_some());
        // Mid-drag the slider leads and the polled value lags behind it.
        if let (Some(value), false) = (value, self.slider.is_pressed()) {
            self.slider.set_value(value);
        }

        let icon_box = self.icon_box(rect);
        if !self.in_bar {
            (self.icon)(status).draw(canvas, icon_box, theme.on_surface);
        }

        let slider_x = icon_box.right + SLIDER_GAP;
        let slider_width = rect.x + rect.width - VALUE_WIDTH - SLIDER_GAP - slider_x;
        self.slider
            .set_layout_rect(LayoutRect::new(slider_x, rect.y, slider_width, rect.height));
        let animating = self.slider.draw(canvas, theme, fonts);

        if value.is_some() {
            let text = format!("{:.0}%", self.slider.value() * 100.0);
            let text_width = fonts.measure(FONT, DETAIL_SIZE, &text);
            let at = Point::new(
                rect.x + rect.width - text_width,
                rect.y + (rect.height + DETAIL_SIZE) / 2.0 - 1.0,
            );
            let mut paint = Paint::new(Color4f::from(theme.on_surface_variant), None);
            paint.set_anti_alias(true);
            fonts.draw(canvas, FONT, DETAIL_SIZE, &text, at, &paint);
        }
        animating
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        self.slider.on_pointer(event)
    }
}

/// A full-width entry picking one of several options, with no icon in the bar. What it
/// shows always comes from `read`, so a failed change snaps back to the real state.
pub struct ChoiceItem {
    read: fn(&SystemStatus) -> Option<usize>,
    available: fn(&SystemStatus, usize) -> bool,
    buttons: SegmentedButton,
}

impl ChoiceItem {
    pub fn new(
        labels: &[&str],
        read: fn(&SystemStatus) -> Option<usize>,
        available: fn(&SystemStatus, usize) -> bool,
        mut on_select: impl FnMut(usize) + 'static,
    ) -> Self {
        let mut buttons = SegmentedButton::new().font(FONT);
        for label in labels {
            buttons.push_segment(Segment::new(*label));
        }
        Self {
            read,
            available,
            buttons: buttons.on_change(move |index, selected| {
                if selected {
                    on_select(index);
                }
            }),
        }
    }
}

impl ToolboxItem for ChoiceItem {
    fn bar_icon(&self, _status: &SystemStatus) -> Option<Icon> {
        None
    }

    fn icon_box(&self, rect: LayoutRect) -> Rect {
        Rect::from_xywh(rect.x, rect.y, 0.0, 0.0)
    }

    fn width(&self) -> DetailWidth {
        DetailWidth::Full
    }

    fn height(&self, _status: &SystemStatus) -> f32 {
        CHOICE_TOP_MARGIN + CHOICE_HEIGHT
    }

    fn draw(
        &mut self,
        canvas: &Canvas,
        rect: LayoutRect,
        state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> bool {
        let status = &state.system;
        let selected = (self.read)(status);
        for index in 0..self.buttons.len() {
            self.buttons.set_selected(index, selected == Some(index));
            self.buttons
                .set_segment_enabled(index, (self.available)(status, index));
        }
        self.buttons.set_layout_rect(LayoutRect::new(
            rect.x,
            rect.y + CHOICE_TOP_MARGIN,
            rect.width,
            CHOICE_HEIGHT,
        ));
        self.buttons.draw(canvas, theme, fonts)
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        self.buttons.on_pointer(event)
    }
}

/// The toolbox's contents. Their order is the order in the bar, and in the panel
/// within each [`DetailWidth`].
pub fn default_items(commands: UnboundedSender<SystemCommand>) -> Vec<Box<dyn ToolboxItem>> {
    let (volume_tx, brightness_tx, profile_tx) = (commands.clone(), commands.clone(), commands);
    vec![
        Box::new(InfoItem::new(
            DetailWidth::Half,
            "Wi-Fi",
            |status| match &status.network {
                Network::Wifi(name) => (Icon::Wifi, name.clone()),
                Network::Wired(name) => (Icon::Ethernet, format!("有線: {name}")),
                Network::Disconnected => (Icon::WifiOff, "未接続".to_string()),
            },
        )),
        Box::new(SliderItem::new(
            0.0,
            |status| match status.volume {
                Some(volume) if !volume.muted => Icon::VolumeUp,
                _ => Icon::VolumeOff,
            },
            |status| status.volume.map(|volume| volume.level.min(1.0)),
            move |value| {
                let _ = volume_tx.send(SystemCommand::Volume(value));
            },
        )),
        Box::new(InfoItem::new(DetailWidth::Half, "Bluetooth", |status| {
            let bluetooth = &status.bluetooth;
            match (bluetooth.powered, bluetooth.connected.as_slice()) {
                (false, _) => (Icon::BluetoothOff, "オフ".to_string()),
                (true, []) => (Icon::Bluetooth, "未接続".to_string()),
                (true, devices) => (Icon::Bluetooth, devices.join(", ")),
            }
        })),
        Box::new(
            SliderItem::new(
                MIN_BRIGHTNESS,
                |_| Icon::Brightness,
                |status| status.brightness,
                move |value| {
                    let _ = brightness_tx.send(SystemCommand::Brightness(value));
                },
            )
            .panel_only(),
        ),
        Box::new(CaptionItem::new(
            |status| match status.battery {
                Some(battery) => Icon::Battery {
                    level: battery.percent,
                    mark: match battery.state {
                        BatteryState::Discharging => BatteryMark::None,
                        BatteryState::Charging => BatteryMark::Bolt,
                        BatteryState::Full => BatteryMark::Check,
                    },
                },
                None => Icon::BatteryMissing,
            },
            |status| {
                status
                    .battery
                    .map(|battery| format!("{}%", battery.percent))
            },
        )),
        Box::new(ChoiceItem::new(
            &["省電力", "バランス", "パフォーマンス"],
            |status| {
                let active = status.power_profile?;
                PowerProfile::ALL.iter().position(|p| *p == active)
            },
            |status, index| status.power_profiles.contains(&PowerProfile::ALL[index]),
            move |index| {
                let _ = profile_tx.send(SystemCommand::PowerProfile(PowerProfile::ALL[index]));
            },
        )),
    ]
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use skia_safe::{surfaces, FontStyle};
    use tokio::sync::mpsc;
    use ui_core::pointer::{button, PointerEventKind};

    use super::*;
    use crate::{
        config::{theme::Theme, WatchableConfig},
        system::Volume,
    };

    fn with_system(system: &SystemStatus) -> UIState {
        UIState {
            system: system.clone(),
            ..Default::default()
        }
    }

    fn volume(level: f32) -> SystemStatus {
        SystemStatus {
            volume: Some(Volume {
                level,
                muted: false,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn choice_sends_a_command_and_shows_only_the_confirmed_state() {
        let theme = Theme::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("example/theme.toml"));
        let mut fonts = FontBook::new();
        fonts.register(FONT, "Noto Sans CJK JP", FontStyle::normal());
        let mut surface = surfaces::raster_n32_premul((400, 100)).unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut item = default_items(tx).pop().unwrap();

        let status = SystemStatus {
            power_profile: Some(PowerProfile::Balanced),
            power_profiles: PowerProfile::ALL.to_vec(),
            ..Default::default()
        };
        let rect = LayoutRect::new(0.0, 0.0, 300.0, CHOICE_TOP_MARGIN + CHOICE_HEIGHT);
        item.draw(
            surface.canvas(),
            rect,
            &with_system(&status),
            &fonts,
            theme.theme(),
        );

        let click = |item: &mut Box<dyn ToolboxItem>, x: f64| {
            for kind in [
                PointerEventKind::Press {
                    button: button::LEFT,
                },
                PointerEventKind::Release {
                    button: button::LEFT,
                },
            ] {
                item.on_pointer(&PointerEvent::new(
                    (x, CHOICE_TOP_MARGIN as f64 + 20.0),
                    kind,
                ));
            }
        };

        click(&mut item, 280.0);
        assert_eq!(
            rx.try_recv(),
            Ok(SystemCommand::PowerProfile(PowerProfile::Performance))
        );

        // Status hasn't changed: redrawing restores Balanced, so a press on it reselects
        // nothing and sends nothing.
        item.draw(
            surface.canvas(),
            rect,
            &with_system(&status),
            &fonts,
            theme.theme(),
        );
        click(&mut item, 150.0);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn slider_sends_commands_and_follows_status_except_mid_drag() {
        let theme = Theme::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("example/theme.toml"));
        let mut fonts = FontBook::new();
        fonts.register(FONT, "Noto Sans CJK JP", FontStyle::normal());
        let mut surface = surfaces::raster_n32_premul((400, 100)).unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let items = default_items(tx);
        let mut item = items.into_iter().nth(1).unwrap();

        let rect = LayoutRect::new(0.0, 0.0, 400.0, SLIDER_HEIGHT);
        let mut draw = |item: &mut Box<dyn ToolboxItem>, status: &SystemStatus| {
            item.draw(
                surface.canvas(),
                rect,
                &with_system(status),
                &fonts,
                theme.theme(),
            );
        };
        draw(&mut item, &volume(0.25));
        let press = PointerEvent::new(
            (200.0, 20.0),
            PointerEventKind::Press {
                button: button::LEFT,
            },
        );
        item.on_pointer(&press);
        let Ok(SystemCommand::Volume(sent)) = rx.try_recv() else {
            panic!("the press should set the volume");
        };
        assert!(sent > 0.25 && sent < 1.0, "sent {sent}");

        // A stale poll arrives mid-drag and must not move the handle.
        draw(&mut item, &volume(0.25));
        let motion = PointerEvent::new((300.0, 20.0), PointerEventKind::Motion);
        item.on_pointer(&motion);
        let Ok(SystemCommand::Volume(dragged)) = rx.try_recv() else {
            panic!("the drag should set the volume");
        };
        assert!(dragged > sent, "dragged from {sent} to {dragged}");
    }
}
