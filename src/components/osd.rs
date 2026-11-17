//! A small island over the middle of the bar that shows the volume or the brightness for a
//! moment after either changes, from the keys or anywhere else. It has no place in the bar.

use std::{rc::Rc, time::Duration};

use anti_webui::prelude::*;
use anti_webui_md3e::{
    components::sized_icon,
    icons::{self, Symbol},
};

use crate::{
    state::State,
    system::Volume,
    ui::{palette, Eased, IslandFrame, IslandPart, IslandPlacement, IslandRequest, Part},
};

const WIDTH: f32 = 280.0;
const HEIGHT: f32 = 56.0;
const PADDING: f32 = 20.0;
const GAP: f32 = 14.0;
const ICON_SIZE: f32 = 24.0;
const TRACK: f32 = 6.0;
const VALUE_WIDTH: f32 = 36.0;
const VALUE_SIZE: f32 = 12.0;
/// What is left of the row for the track.
const TRACK_WIDTH: f32 = WIDTH - PADDING * 2.0 - ICON_SIZE - VALUE_WIDTH - GAP * 2.0;
/// Below media's, which the user opened on purpose, and the toolbox, whose sliders already
/// show what changed.
const PRIORITY: u8 = 20;
const SHOWN_FOR: Duration = Duration::from_millis(1500);
/// The same curve as the toolbox's sliders.
const EASING: &str = "slider_level";
const GLIDE: Duration = Duration::from_millis(250);

/// What changed: an icon and a level from 0 to 1.
type Level = (&'static Symbol, f32);

fn volume(v: Volume) -> Level {
    if v.muted { (icons::VOLUME_OFF, 0.0) } else { (icons::VOLUME_UP, v.level.min(1.0)) }
}

pub fn osd(state: &State) -> Part {
    let system = state.system;
    let levels = memo(move || system.with(|s| (s.volume, s.brightness)));
    // Kept while the island collapses, so it has something to show.
    let shown = signal((icons::VOLUME_UP, 0.0f32));
    let open = signal(false);
    let pulse = signal(0u64);
    let fill = Eased::new(state, EASING, GLIDE, 0.0);
    let last = std::cell::Cell::new(None);
    effect(move || {
        let now = levels.get();
        // The first values, and a device appearing, aren't changes anyone made.
        let Some(before) = last.replace(Some(now)) else { return };
        let next = match (before, now) {
            ((Some(a), _), (Some(b), _)) if a != b => volume(b),
            ((_, Some(a)), (_, Some(b))) if a != b => (icons::LIGHT_MODE, b),
            _ => return,
        };
        untrack(|| {
            // The same level glides from where it was; a first one, or the other level, starts
            // where it is.
            let is_light = |l: Level| std::ptr::eq(l.0, icons::LIGHT_MODE);
            if pulse.get() > 0 && is_light(shown.get()) == is_light(next) {
                fill.to(next.1);
            } else {
                fill.set(next.1);
            }
            shown.set(next);
            open.set(true);
            pulse.update(|p| *p += 1);
        });
    });
    // Each change restarts the countdown: the timer goes with the run that set it.
    effect(move || {
        if pulse.get() > 0 {
            set_timeout(SHOWN_FOR, move || open.set(false));
        }
    });

    let request = memo(move || {
        open.get().then_some(IslandRequest {
            width: WIDTH,
            height: HEIGHT,
            placement: IslandPlacement::Joined { center_x: None },
            priority: PRIORITY,
            passive: true,
        })
    });

    let (fg, track, fill_color) = (state.clone(), state.clone(), state.clone());
    let content = div()
        .w(WIDTH)
        .h(HEIGHT)
        .px(PADDING)
        .flex()
        .flex_row()
        .items_center()
        .gap(GAP)
        .dyn_style(move |s| s.text_color(palette(&fg, |t| t.on_surface)))
        .child(dyn_view(move || sized_icon(shown.get().0, ICON_SIZE).into_view()))
        .child(
            div()
                .w(TRACK_WIDTH)
                .h(TRACK)
                .rounded(TRACK / 2.0)
                .overflow_hidden()
                .dyn_style(move |s| s.bg(palette(&track, |t| t.surface_container_highest)))
                .child(div().h_full().rounded(TRACK / 2.0).dyn_style(move |s| {
                    s.w(TRACK_WIDTH * fill.get()).bg(palette(&fill_color, |t| t.primary))
                })),
        )
        .child(
            div()
                .w(VALUE_WIDTH)
                .flex()
                .justify_end()
                .child(text(move || format!("{:.0}%", shown.get().1 * 100.0)).text_size(VALUE_SIZE)),
        );

    Part {
        bar: None,
        island: Some(IslandPart {
            request,
            content: content.into_view(),
            close: Rc::new(move || open.set(false)),
            frame: signal(None::<IslandFrame>),
        }),
        overlay: None,
    }
}
