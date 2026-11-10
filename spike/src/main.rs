//! Phase 0 of the AntiWebUI migration: can the bar's shape, input region and data flow be built
//! on it? One `Top` layer per output, a bar with an island growing out of it as one silhouette,
//! clicks through the transparent headroom, and a thread feeding a signal. See TODO.md.
//!
//! `cargo run -p spike --release`

use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use kurbo::BezPath;
use std::time::Duration;

const BAR: f32 = 48.0;
/// Transparent room above the bar for the island to grow into.
const HEADROOM: f32 = 200.0;
const CLOSED: (f32, f32) = (160.0, 0.0);
const OPEN: (f32, f32) = (420.0, 150.0);
const CORNER: f32 = 24.0;
const FILLET: f32 = 16.0;
const FLUSH: f32 = 0.5;
/// Cubic control distance for a quarter circle, standing in for Skia's √2/2 conic.
const ARC: f64 = 0.552_284_8;

fn main() {
    let result = Shell::new().namespace("my_shell_spike").run(|| {
        // Stands in for the D-Bus/IPC threads: they keep their threads and send here.
        let ticks = signal(0u64);
        let tx = ui_channel(move |n: u64| ticks.set(n));
        std::thread::spawn(move || {
            for n in 1.. {
                std::thread::sleep(Duration::from_secs(1));
                tx.send(n);
            }
        });
        for_each_output(move |output| bar_layer(output, ticks));
    });
    if let Err(e) = result {
        eprintln!("{e}");
    }
}

fn bar_layer(output: Output, ticks: Signal<u64>) {
    let width = output.logical_size.width as f32;
    let open = signal(false);
    let size = animated(CLOSED);
    // SPIKE_AUTO=1 toggles the island every 3 s, for screenshots and measuring without a pointer.
    if std::env::var_os("SPIKE_AUTO").is_some() {
        effect(move || {
            let n = ticks.get();
            if n > 0 && n % 3 == 0 {
                open.update(|o| *o = !*o);
            }
        });
    }
    effect(move || {
        let target = if open.get() { OPEN } else { CLOSED };
        size.to(target, Transition::ms(350).cubic_bezier(0.2, 0.0, 0.0, 1.0));
    });

    let layer = open_layer(
        LayerOptions::new()
            .layer(Layer::Top)
            .anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT)
            .height((BAR + HEADROOM) as i32)
            .exclusive(Exclusive::Fixed(BAR as i32))
            .output(&output),
        move || view(width, size, open, ticks),
    );
    // Only the bar and the island take clicks; the headroom around the island lets them through.
    effect(move || {
        let (w, h) = size.get();
        let island = island_rect(width, w, h);
        layer.set_input_region(InputRegion::Rects(vec![
            RectI::new(0, HEADROOM as i32, width as i32, BAR as i32),
            RectI::new(
                island.x0.floor() as i32,
                island.y0.floor() as i32,
                island.width().ceil() as i32,
                island.height().ceil() as i32 + 1,
            ),
        ]));
    });
}

fn view(width: f32, size: Animated<(f32, f32)>, open: Signal<bool>, ticks: Signal<u64>) -> Div {
    let shape = svg().w(width).h(BAR + HEADROOM).child(
        svg::path(move || {
            let (w, h) = size.get();
            island_path(width, island_rect(width, w, h))
        })
        .fill(rgb(0x1d1b20))
        .on_click(move |_| open.update(|o| *o = !*o)),
    );
    let clock = text(move || format!("tick {}", ticks.get()))
        .font_family("Roboto")
        .text_size(16.0)
        .text_color(rgb(0xe6e0e9));
    // Shrinks and fades with the island, as the bar's island content scales with its morph.
    let content = text(move || format!("日本語のテスト — {} 秒", ticks.get()))
        .font_family("Noto Sans CJK JP")
        .text_size(20.0)
        .text_color(rgb(0xe6e0e9));
    div()
        .relative()
        .w(width)
        .h(BAR + HEADROOM)
        .child(shape)
        .child(
            div()
                .absolute()
                .left(0.0)
                .bottom(0.0)
                .w(width)
                .h(BAR)
                .flex()
                .items_center()
                .justify_center()
                .pointer_events_none()
                .child(clock),
        )
        .child(
            div()
                .absolute()
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .pointer_events_none()
                .dyn_style(move |s| {
                    let (w, h) = size.get();
                    let r = island_rect(width, w, h);
                    s.left(r.x0 as f32)
                        .top(r.y0 as f32)
                        .w(r.width() as f32)
                        .h(r.height() as f32)
                        .opacity((h / OPEN.1).clamp(0.0, 1.0))
                })
                .child(content),
        )
}

/// The island, centered, standing on the bar's top edge.
fn island_rect(bar_width: f32, w: f32, h: f32) -> kurbo::Rect {
    let x = ((bar_width - w) / 2.0) as f64;
    kurbo::Rect::new(x, (HEADROOM - h) as f64, x + w as f64, HEADROOM as f64)
}

/// `ui_core::island::island_path`, on kurbo: the bar with the island growing from its top edge.
fn island_path(bar_width: f32, island: kurbo::Rect) -> BezPath {
    let (bar_x, top, bar_right, bar_bottom) =
        (0.0, HEADROOM as f64, bar_width as f64, (HEADROOM + BAR) as f64);
    let (left, right, island_top) = (island.x0, island.x1, island.y0);
    let half_height = (island.height() / 2.0).max(0.0);
    let corner = (CORNER as f64).min(half_height).min(island.width() / 2.0);
    let left_corner = if left - bar_x < FLUSH as f64 { 0.0 } else { corner };
    let right_corner = if bar_right - right < FLUSH as f64 { 0.0 } else { corner };
    let fillet = (FILLET as f64).min(half_height);
    let left_fillet = fillet.min((left - bar_x).max(0.0));
    let right_fillet = fillet.min((bar_right - right).max(0.0));

    let mut p = BezPath::new();
    // A quarter circle from `from` around `corner` to `to`.
    let arc = |p: &mut BezPath, from: (f64, f64), corner: (f64, f64), to: (f64, f64)| {
        let lerp = |a: (f64, f64), b: (f64, f64)| (a.0 + (b.0 - a.0) * ARC, a.1 + (b.1 - a.1) * ARC);
        p.curve_to(lerp(from, corner), lerp(to, corner), to);
    };
    p.move_to((bar_x, top));
    p.line_to((left - left_fillet, top));
    arc(&mut p, (left - left_fillet, top), (left, top), (left, top - left_fillet));
    p.line_to((left, island_top + left_corner));
    arc(&mut p, (left, island_top + left_corner), (left, island_top), (left + left_corner, island_top));
    p.line_to((right - right_corner, island_top));
    arc(&mut p, (right - right_corner, island_top), (right, island_top), (right, island_top + right_corner));
    p.line_to((right, top - right_fillet));
    arc(&mut p, (right, top - right_fillet), (right, top), (right + right_fillet, top));
    p.line_to((bar_right, top));
    p.line_to((bar_right, bar_bottom));
    p.line_to((bar_x, bar_bottom));
    p.close_path();
    p
}
