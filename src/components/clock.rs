use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;

use crate::{
    state::State,
    ui::{palette, BarSlot, Part},
};

const FORMAT: &str = "%H:%M:%S";
const FONT_SIZE: f32 = 32.0;

pub fn clock(state: &State) -> Part {
    let state = state.clone();
    let view = text(|| local_time(Every::Second).format(FORMAT).to_string())
        .text_size(FONT_SIZE)
        .whitespace_nowrap()
        .dyn_style(move |s| s.text_color(palette(&state, |t| t.primary)));
    Part {
        bar: Some((BarSlot::Left, view.into_view())),
        island: None,
    }
}
