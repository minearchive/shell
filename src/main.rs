use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use clap::Parser;

use crate::{
    cli::{Cli, Command},
    config::set_config_dir,
    state::State,
    system::{BatteryState, Network},
};

mod cli;
mod config;
mod dbus;
mod ipc;
mod state;
mod system;

// ponytail: the Skia UI (`ui`, `components`, `banner`, `catcher`, `dnd`, `output`, `seat`) is
// out of the build while Phase 2 of TODO.md ports it to AntiWebUI; the files stay as reference.

pub const BAR_HEIGHT: i32 = 48;

fn main() {
    env_logger::init();

    let cli = Cli::parse();
    if let Some(Command::Action { action }) = cli.command {
        if let Err(e) = cli::send(&action) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    if let Some(dir) = cli.config_directory {
        set_config_dir(dir);
    }

    let result = Shell::new().namespace("my_shell").run(|| {
        let state = match State::start() {
            Ok(state) => state,
            Err(e) => {
                eprintln!("{e}");
                quit();
                return;
            }
        };
        for_each_output(move |output| {
            let state = state.clone();
            open_layer(
                LayerOptions::new()
                    .layer(Layer::Top)
                    .anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT)
                    .height(BAR_HEIGHT)
                    .exclusive(Exclusive::Fixed(BAR_HEIGHT))
                    .output(&output),
                move || bar(state),
            );
        });
    });
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn color(c: ui_core::scheme::color::Color) -> Color {
    Color::new(c.r, c.g, c.b, c.a)
}

/// ponytail: Phase 1's stand-in bar, one line of what every provider reports, to see the
/// signals arrive. Phase 2 replaces it with the components.
fn bar(state: State) -> Div {
    let State {
        players,
        last_playing,
        warp,
        notifications,
        system,
        focused_title,
        focused_workspace,
        theme,
        action,
        ..
    } = state;
    let item = |f: Box<dyn Fn() -> String>| text(f).whitespace_nowrap();
    div()
        .size_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(16.0)
        .px(16.0)
        .font_family("Noto Sans CJK JP")
        .text_size(14.0)
        .dyn_style(move |s| {
            theme.with(|t| {
                s.bg(color(t.theme().surface_container))
                    .text_color(color(t.theme().on_surface))
            })
        })
        .child(item(Box::new(|| {
            local_time(Every::Minute).format("%H:%M").to_string()
        })))
        .child(item(Box::new(move || {
            let ws = focused_workspace.get().map_or("-".into(), |w| w.to_string());
            format!("ws {ws} · {}", focused_title.get().unwrap_or_default())
        })))
        .child(div().flex_1())
        .child(item(Box::new(move || {
            let playing = last_playing.get().and_then(|bus| {
                players.with(|p| p.get(&bus).and_then(|p| p.title.clone()))
            });
            playing.map_or(String::new(), |t| format!("♪ {t}"))
        })))
        .child(item(Box::new(move || {
            system.with(|s| {
                let net = match &s.network {
                    Network::Disconnected => "offline".to_string(),
                    Network::Wifi(ssid) => ssid.clone(),
                    Network::Wired(name) => name.clone(),
                };
                let battery = s.battery.map_or(String::new(), |b| {
                    let plug = if b.state == BatteryState::Discharging { "" } else { "+" };
                    format!(" · {}%{plug}", b.percent)
                });
                let volume = s
                    .volume
                    .map_or(String::new(), |v| format!(" · vol {:.0}%", v.level * 100.0));
                format!("{net}{battery}{volume}")
            })
        })))
        .child(item(Box::new(move || {
            warp.get()
                .map_or(String::new(), |w| format!("warp {}", w.status))
        })))
        .child(item(Box::new(move || {
            let (on_display, unseen) = notifications.with(|n| (n.len(), n.unseen_len()));
            let last = action.get().map_or(String::new(), |(_, a)| format!(" · {a:?}"));
            format!("通知 {on_display}/{unseen}{last}")
        })))
}
