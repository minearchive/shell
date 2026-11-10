use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use clap::Parser;

use crate::{
    cli::{Cli, Command},
    config::set_config_dir,
    state::State,
    ui::{IslandPart, IslandPlacement, IslandRequest, Part, BAR_HEIGHT, ISLAND_MAX_HEIGHT},
};

mod cli;
mod components;
mod config;
mod dbus;
mod ipc;
mod state;
mod system;
mod ui;

// ponytail: the rest of the Skia UI (`banner`, `catcher`, `dnd`, `output`, `seat`, and the
// components not yet in `components/mod.rs`) is out of the build while Phase 2 of TODO.md
// ports it to AntiWebUI; the files stay as reference.

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
            let width = output.logical_size.width as f32;
            let input = signal(Vec::new());
            let layer = open_layer(
                LayerOptions::new()
                    .layer(Layer::Top)
                    .anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT)
                    .height((BAR_HEIGHT + ISLAND_MAX_HEIGHT) as i32)
                    .exclusive(Exclusive::Fixed(BAR_HEIGHT as i32))
                    .input_region(InputRegion::Empty)
                    .output(&output),
                move || {
                    let mut parts = vec![components::clock::clock(&state)];
                    if std::env::var_os("MY_SHELL_DEBUG_ISLAND").is_some() {
                        parts.push(debug_island(&state));
                    }
                    ui::bar(&state, width, parts, input)
                },
            );
            effect(move || layer.set_input_region(InputRegion::Rects(input.get())));
        });
    });
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

/// ponytail: opens on every `my_shell action launcher`, to try the islands before a real
/// component bids for one. Goes when media is ported.
fn debug_island(state: &State) -> Part {
    let action = state.action;
    let request = memo(move || {
        let open = action.get().is_some_and(|(n, _)| n % 2 == 0);
        open.then_some(IslandRequest {
            width: 420.0,
            height: 160.0,
            placement: IslandPlacement::Joined { center_x: None },
            priority: 100,
        })
    });
    let state = state.clone();
    let content = div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(20.0)
        .dyn_style(move |s| s.text_color(ui::palette(&state, |t| t.on_surface)))
        .child(text("アイランドのテスト"));
    Part {
        bar: None,
        island: Some(IslandPart {
            request,
            content: content.into_view(),
        }),
    }
}
