//! What the toolbox shows. To add an entry, push one to [`items`]: most are a [`tile`] or a
//! [`slider_row`].

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use anti_webui::prelude::*;
use anti_webui_md3e::{
    components::{
        connected_button_group, sized_icon, styled_slider, styled_switch, ButtonStyle, SliderStyle,
        SwitchStyle,
    },
    icons::{self, Symbol},
};
use ui_core::util::elide;

use crate::{
    dbus::warp::WarpCommand,
    state::State,
    system::{BatteryState, Network, PowerProfile, SystemCommand, SystemStatus},
    ui::{measure, palette},
};

pub const ICON_SIZE: f32 = 24.0;
const INFO_HEIGHT: f32 = 56.0;
const INFO_RADIUS: f32 = 16.0;
const INFO_PADDING: f32 = 12.0;
const TITLE_SIZE: f32 = 14.0;
const DETAIL_SIZE: f32 = 12.0;
const SLIDER_HEIGHT: f32 = 44.0;
const SLIDER_GAP: f32 = 12.0;
const VALUE_WIDTH: f32 = 40.0;
const CHOICE_HEIGHT: f32 = 40.0;
/// On top of the panel's gap: the buttons fill their row, unlike the thin sliders.
const CHOICE_TOP_MARGIN: f32 = 6.0;
const CAPTION_HEIGHT: f32 = 16.0;
/// Keeps a dragged-down backlight from turning the screen fully black.
const MIN_BRIGHTNESS: f32 = 0.01;
/// How long a slider ignores the polled value after it was moved, so a stale poll can't pull
/// the handle back mid-drag.
const SLIDER_LEAD: Duration = Duration::from_millis(600);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailWidth {
    /// A small line across the top of the panel, above everything else.
    Caption,
    /// Half the panel. Half-width entries pair up in rows at the top of the panel.
    Half,
    /// The panel's full width, stacked below the half-width rows.
    Full,
}

pub struct Item {
    pub width: DetailWidth,
    /// Zero leaves the entry out of the panel, gap included.
    pub height: fn(&SystemStatus) -> f32,
    /// The icon for the row in the bar. `None` keeps this entry to the panel.
    pub bar_icon: fn(&SystemStatus) -> Option<&'static Symbol>,
    /// Fills the rect the panel gives it.
    pub view: View,
}

fn info_height(_: &SystemStatus) -> f32 {
    INFO_HEIGHT
}

fn no_bar_icon(_: &SystemStatus) -> Option<&'static Symbol> {
    None
}

fn icon(symbol: impl Fn() -> &'static Symbol + 'static, state: &State) -> View {
    let state = state.clone();
    div()
        .flex_none()
        .dyn_style(move |s| s.text_color(palette(&state, |t| t.on_surface)))
        .child(dyn_view(move || sized_icon(symbol(), ICON_SIZE).into_view()))
        .into_view()
}

/// A tile: an icon, a title and one line of detail, and `trailing` at its end.
fn tile(
    state: &State,
    title: &'static str,
    read: impl Fn(&SystemStatus) -> (&'static Symbol, String) + Clone + 'static,
    trailing: Option<View>,
) -> View {
    let system = state.system;
    let symbol = {
        let read = read.clone();
        move || system.with(|s| read(s).0)
    };
    let detail_width = signal(0.0f32);
    let detail = move || system.with(|s| elide(&read(s).1, detail_width.get(), measure(DETAIL_SIZE)));
    let (on_surface, on_surface_variant) = {
        let (a, b) = (state.clone(), state.clone());
        (move || palette(&a, |t| t.on_surface), move || palette(&b, |t| t.on_surface_variant))
    };
    let bg = state.clone();
    let mut row = div()
        .size_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(INFO_PADDING)
        .px(INFO_PADDING)
        .rounded(INFO_RADIUS)
        .dyn_style(move |s| s.bg(palette(&bg, |t| t.surface_container_high)))
        .child(icon(symbol, state))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(0.0)
                .on_resized(move |w, _| detail_width.set(w))
                .child(text(title).text_size(TITLE_SIZE).dyn_style(move |s| s.text_color(on_surface())))
                .child(
                    text(detail)
                        .text_size(DETAIL_SIZE)
                        .whitespace_nowrap()
                        .dyn_style(move |s| s.text_color(on_surface_variant())),
                ),
        );
    if let Some(trailing) = trailing {
        row = row.child(trailing);
    }
    row.into_view()
}

/// A full-width entry the user adjusts: an icon, a slider and the percentage. Disabled while
/// `read` returns `None`.
fn slider_row(
    state: &State,
    min: f32,
    symbol: fn(&SystemStatus) -> &'static Symbol,
    read: fn(&SystemStatus) -> Option<f32>,
    command: fn(f32) -> SystemCommand,
) -> View {
    let system = state.system;
    let value = signal(min);
    // ponytail: the polled value is ignored for a moment after a move rather than for exactly
    // as long as the handle is held; the slider doesn't say when a drag ends.
    let moved = Rc::new(Cell::new(None::<Instant>));
    {
        let moved = moved.clone();
        effect(move || {
            let polled = system.with(read);
            let leading = moved.get().is_some_and(|at| at.elapsed() < SLIDER_LEAD);
            if let (Some(polled), false) = (polled, leading) {
                untrack(|| value.set(polled.max(min)));
            }
        });
    }
    let send = state.commands.system.clone();
    let binding = Binding::new(
        move || value.get(),
        move |v: f32| {
            value.set(v);
            moved.set(Some(Instant::now()));
            let _ = send.send(command(v));
        },
    );
    let style = SliderStyle::new()
        .range(min..=1.0)
        .disabled_when(move || system.with(|s| read(s).is_none()));
    let variant = state.clone();
    div()
        .size_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(SLIDER_GAP)
        .child(icon(move || system.with(symbol), state))
        .child(styled_slider(binding, style).flex_1().min_w(0.0))
        .child(
            div().w(VALUE_WIDTH).flex_none().flex().justify_end().child(
                text(move || {
                    if system.with(|s| read(s).is_some()) { format!("{:.0}%", value.get() * 100.0) } else { String::new() }
                })
                .text_size(DETAIL_SIZE)
                .dyn_style(move |s| s.text_color(palette(&variant, |t| t.on_surface_variant))),
            ),
        )
        .into_view()
}

/// The toolbox's contents. Their order is the order in the bar, and in the panel within each
/// [`DetailWidth`].
pub fn items(state: &State) -> Vec<Item> {
    let system = state.system;
    let warp = state.warp;
    let warp_commands = state.commands.warp.clone();
    let profile_commands = state.commands.system.clone();
    let variant = state.clone();

    fn network(status: &SystemStatus) -> (&'static Symbol, String) {
        match &status.network {
            Network::Wifi(name) => (icons::WIFI, name.clone()),
            Network::Wired(name) => (icons::LAN, format!("有線: {name}")),
            Network::Disconnected => (icons::WIFI_OFF, "未接続".to_string()),
        }
    }
    fn bluetooth(status: &SystemStatus) -> (&'static Symbol, String) {
        let bluetooth = &status.bluetooth;
        match (bluetooth.powered, bluetooth.connected.as_slice()) {
            (false, _) => (icons::BLUETOOTH_DISABLED, "オフ".to_string()),
            (true, []) => (icons::BLUETOOTH, "未接続".to_string()),
            (true, devices) => (icons::BLUETOOTH, devices.join(", ")),
        }
    }
    fn volume(status: &SystemStatus) -> &'static Symbol {
        match status.volume {
            Some(volume) if !volume.muted => icons::VOLUME_UP,
            _ => icons::VOLUME_OFF,
        }
    }
    fn battery(status: &SystemStatus) -> &'static Symbol {
        const LEVELS: [&Symbol; 7] = [
            icons::BATTERY_ANDROID_0,
            icons::BATTERY_ANDROID_1,
            icons::BATTERY_ANDROID_2,
            icons::BATTERY_ANDROID_3,
            icons::BATTERY_ANDROID_4,
            icons::BATTERY_ANDROID_5,
            icons::BATTERY_ANDROID_6,
        ];
        match status.battery {
            Some(b) if b.state == BatteryState::Charging => icons::BATTERY_ANDROID_BOLT,
            Some(b) if b.state == BatteryState::Full || b.percent >= 100 => icons::BATTERY_ANDROID_FULL,
            Some(b) => LEVELS[(usize::from(b.percent) * LEVELS.len() / 100).min(LEVELS.len() - 1)],
            None => icons::BATTERY_ANDROID_QUESTION,
        }
    }

    // WARP: what the switch shows always comes from the status, so a failed change snaps back.
    let warp_on = Binding::new(
        move || warp.with(|w| w.as_ref().is_some_and(|w| w.is_connected() || w.is_connecting())),
        move |on: bool| {
            let _ = warp_commands.send(if on { WarpCommand::Connect } else { WarpCommand::Disconnect });
        },
    );
    let warp_switch = styled_switch(
        warp_on,
        SwitchStyle::new().disabled_when(move || warp.with(|w| w.as_ref().is_none_or(|w| w.is_connecting()))),
    )
    .flex_none()
    .into_view();
    let warp_tile = {
        let warp_state = state.clone();
        let detail = move |_: &SystemStatus| {
            let detail = warp_state.warp.with(|w| match w.as_ref().map(|w| w.status.as_str()) {
                None => "利用不可".to_string(),
                Some("Connected") => "接続済み".to_string(),
                Some("Connecting") => "接続中".to_string(),
                Some("Disconnected") => "未接続".to_string(),
                Some(other) => other.to_string(),
            });
            (icons::SHIELD, detail)
        };
        tile(state, "WARP", detail, Some(warp_switch))
    };

    let profiles = connected_button_group(
        ["省電力", "バランス", "パフォーマンス"],
        ButtonStyle::outlined(),
        move |i| system.with(|s| s.power_profile == Some(PowerProfile::ALL[i])),
        move |i| {
            let profile = PowerProfile::ALL[i];
            if system.with(|s| s.power_profiles.contains(&profile)) {
                let _ = profile_commands.send(SystemCommand::PowerProfile(profile));
            }
        },
    );

    vec![
        Item {
            width: DetailWidth::Half,
            height: info_height,
            bar_icon: |s| Some(network(s).0),
            view: tile(state, "Wi-Fi", network, None),
        },
        Item {
            width: DetailWidth::Full,
            height: info_height,
            bar_icon: no_bar_icon,
            view: warp_tile,
        },
        Item {
            width: DetailWidth::Full,
            height: |_| SLIDER_HEIGHT,
            bar_icon: |s| Some(volume(s)),
            view: slider_row(state, 0.0, volume, |s| s.volume.map(|v| v.level.min(1.0)), SystemCommand::Volume),
        },
        Item {
            width: DetailWidth::Half,
            height: info_height,
            bar_icon: |s| Some(bluetooth(s).0),
            view: tile(state, "Bluetooth", bluetooth, None),
        },
        Item {
            width: DetailWidth::Full,
            height: |_| SLIDER_HEIGHT,
            bar_icon: no_bar_icon,
            view: slider_row(state, MIN_BRIGHTNESS, |_| icons::LIGHT_MODE, |s| s.brightness, SystemCommand::Brightness),
        },
        Item {
            width: DetailWidth::Caption,
            height: |s| if s.battery.is_some() { CAPTION_HEIGHT } else { 0.0 },
            bar_icon: |s| Some(battery(s)),
            view: div()
                .size_full()
                .flex()
                .justify_end()
                .items_center()
                .child(
                    text(move || system.with(|s| s.battery.map(|b| format!("{}%", b.percent)).unwrap_or_default()))
                        .text_size(DETAIL_SIZE)
                        .dyn_style(move |s| s.text_color(palette(&variant, |t| t.on_surface_variant))),
                )
                .into_view(),
        },
        Item {
            width: DetailWidth::Full,
            height: |_| CHOICE_TOP_MARGIN + CHOICE_HEIGHT,
            bar_icon: no_bar_icon,
            view: div().size_full().pt(CHOICE_TOP_MARGIN).flex().child(profiles.flex_1()).into_view(),
        },
    ]
}
