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

// ponytail: the rest of the Skia UI (`dnd`, `output`, `seat`, and the
// components not yet in `components/mod.rs`) is out of the build while Phase 2 of TODO.md
// ports it to AntiWebUI; the files stay as reference.

/// While an island is open, a transparent layer over the whole screen but the bar and the island
/// closes it on a press: Wayland shows a client nothing of other surfaces' input, so the only way
/// to see a press elsewhere is to be what it lands on. `input` is the bar's, in its surface.
fn catch_outside_presses(bar: &ui::Bar, input: Signal<Vec<RectI>>, output: Output, height: i32) {
    let (open, close) = (bar.open, bar.close.clone());
    effect(move || {
        if !open.get() {
            return;
        }
        // Closed with this run of the effect, when the island closes.
        let close = close.clone();
        let catcher = open_layer(
            LayerOptions::new()
                .layer(Layer::Top)
                .anchor(Anchor::ALL)
                // Over the bar's exclusive zone too, so the holes line up with it.
                .exclusive(Exclusive::Ignore)
                .input_region(InputRegion::Empty)
                .output(&output),
            move || div().size_full().on_pointer_down(move |_| close()),
        );
        let width = output.logical_size.width;
        let surface_top = height - (BAR_HEIGHT + ISLAND_MAX_HEIGHT) as i32;
        effect(move || {
            let holes: Vec<_> = input.get().into_iter().map(|r| RectI::new(r.x, r.y + surface_top, r.width, r.height)).collect();
            catcher.set_input_region(InputRegion::Rects(ui::cut_out(RectI::new(0, 0, width, height), &holes)));
        });
    });
}

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
        let theme = state.theme;
        effect(move || anti_webui_md3e::set_color_scheme(theme.with(|t| ui::md3e_scheme(t.theme()))));
        for_each_output(move |output| {
            let (state, banner_state) = (state.clone(), state.clone());
            let width = output.logical_size.width as f32;
            let input = signal(Vec::new());
            let catcher_output = output.clone();
            let height = output.logical_size.height;
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
                        components::toolbox::toolbox(&state),
                    ];
                    let bar = ui::bar(&state, width, parts, input);
                    catch_outside_presses(&bar, input, catcher_output, height);
                    bar.view
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
