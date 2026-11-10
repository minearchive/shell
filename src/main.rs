use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use clap::Parser;

use crate::{
    cli::{Cli, Command},
    config::set_config_dir,
    state::State,
    ui::{BAR_HEIGHT, ISLAND_MAX_HEIGHT},
};

mod cli;
mod components;
mod config;
mod dbus;
mod ipc;
mod state;
mod system;
mod ui;

// ponytail: the rest of the Skia UI (`catcher`, `dnd`, `output`, `seat`, and the
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
            let (state, banner_state) = (state.clone(), state.clone());
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
                    let parts = vec![
                        components::clock::clock(&state),
                        components::media::media(&state, width),
                    ];
                    ui::bar(&state, width, parts, input)
                },
            );
            effect(move || layer.set_input_region(InputRegion::Rects(input.get())));

            // The notification banner, above the catcher's layer so it keeps its clicks.
            let input = signal(Vec::new());
            let layer = open_layer(
                LayerOptions::new()
                    .layer(Layer::Overlay)
                    .anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT)
                    .height(components::notifications::SURFACE_HEIGHT as i32)
                    .input_region(InputRegion::Empty)
                    .output(&output),
                move || components::notifications::banner(&banner_state, width, input),
            );
            effect(move || layer.set_input_region(InputRegion::Rects(input.get())));
        });
    });
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
