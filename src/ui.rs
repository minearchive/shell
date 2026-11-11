//! The bar and its islands, on AntiWebUI. Every component is a [`Part`]: what it puts in the bar,
//! and its bid for an island with what the island shows. [`bar`] lays the parts out, draws the
//! bar and every visible island as one silhouette, and morphs each island with its own
//! [`IslandMorph`], so one can open while another collapses.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use anti_webui::prelude::*;
use kurbo::BezPath;
use ui_core::{
    animation::animation::{easing::ease_out_cubic, Animation},
    geometry::LayoutRect,
    island::IslandMorph,
    scheme::ColorTheme,
};

use crate::state::State;

pub const BAR_HEIGHT: f32 = 48.0;
/// Headroom above the bar that islands can grow into. The surface always includes it,
/// transparent and click-through, so growing never waits on a configure.
pub const ISLAND_MAX_HEIGHT: f32 = 400.0;

const BAR_TOP: f32 = ISLAND_MAX_HEIGHT;
const ISLAND_CORNER: f32 = 28.0;
const ISLAND_FILLET: f32 = 20.0;
/// Keeps room beside an island at the bar's edge for its fillet.
pub const ISLAND_SIDE_MARGIN: f32 = 24.0;
const ISLAND_MORPH: Duration = Duration::from_millis(350);
const ISLAND_EASING_ID: &str = "island";
/// Space before the first `Left` part. `Right` parts run to the screen's edge, so one can
/// attach to it.
const BAR_PADDING: f32 = 20.0;
const BAR_GAP: f32 = 16.0;
/// Cubic control distance for a quarter circle, standing in for Skia's √2/2 conic.
const ARC: f64 = 0.552_284_8;
const FONT: &str = "Noto Sans CJK JP";

/// Where an island rises from the bar's top edge, joined to it by curves.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)] // ponytail: Phase 2 ports the components that use the rest.
pub enum IslandPlacement {
    /// Over a point on the bar, scaling up from nothing at its bottom center. `center_x` of
    /// `None` centers it on the bar.
    Joined { center_x: Option<f32> },
    /// Centered like `Joined`, but reaching down over the bar to the screen's edge, taking
    /// over the middle of the bar's row for as long as it is open.
    OverBar,
    /// Flush with the screen's right edge, widening from `from_width` and scaling its
    /// content from the bottom-right.
    Corner { from_width: f32 },
}

/// A part's bid for the bar's main island. Clamped to the available space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IslandRequest {
    /// The size the content is laid out at.
    pub width: f32,
    pub height: f32,
    pub placement: IslandPlacement,
    /// The highest priority wins. On a tie, the later part does.
    pub priority: u8,
}

/// Which end of the bar a part sits at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum BarSlot {
    Left,
    /// Centered on the bar, whatever is at either end.
    Center,
    Right,
}

/// What a component adds to the bar.
pub struct Part {
    /// Its place in the bar and what it shows there, if anything.
    pub bar: Option<(BarSlot, View)>,
    pub island: Option<IslandPart>,
    /// Drawn over the bar and every island, from the top-left of the part's place in the bar:
    /// for what travels between the bar and the island as it opens, following
    /// [`IslandPart::frame`].
    pub overlay: Option<View>,
}

/// A part's island as of this frame, from the top-left of the part's place in the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IslandFrame {
    /// How far open it is: 0 at rest, 1 fully open.
    pub progress: f32,
    /// Where its content sits once fully open.
    pub content: LayoutRect,
}

/// A component's island.
pub struct IslandPart {
    /// Read reactively: return a request for as long as you want the main island. Only the
    /// winner's stays open: the others collapse until they win again.
    pub request: Memo<Option<IslandRequest>>,
    /// Laid out at the request's full size; the island scales and fades it while morphing.
    /// It stays shown while the island collapses, after the request has gone, so it should
    /// hold on to what it was showing.
    pub content: View,
    /// Withdraws the request, for a press outside the bar and the island.
    pub close: Rc<dyn Fn()>,
    /// Written by [`bar`]: the island while it shows, `None` otherwise.
    pub frame: Signal<Option<IslandFrame>>,
}

/// What [`bar`] makes: its view, whether an island is open, and what closes every island.
pub struct Bar {
    pub view: Div,
    pub open: Memo<bool>,
    pub close: Rc<dyn Fn()>,
}

/// Where an island is going.
#[derive(Clone, Copy, Debug)]
struct IslandSlot {
    target: LayoutRect,
    placement: IslandPlacement,
}

impl IslandSlot {
    /// Where content goes in `rect`: `(x, y, scale)`, scaled uniformly so it never distorts
    /// or overflows, from its bottom-right for a corner island and its bottom center
    /// otherwise. The scale doubles as how far open the island is.
    fn content_transform(&self, rect: LayoutRect) -> (f32, f32, f32) {
        let (width, height) = (self.target.width, self.target.height);
        let scale = (rect.width / width).min(rect.height / height).min(1.0);
        let x = match self.placement {
            IslandPlacement::Joined { .. } | IslandPlacement::OverBar => {
                rect.x + (rect.width - width * scale) / 2.0
            }
            IslandPlacement::Corner { .. } => rect.x + rect.width - width * scale,
        };
        (x, rect.y + rect.height - height * scale, scale)
    }
}

/// A part's own island, which opens and collapses where it is, whatever the others do.
struct Island {
    request: Memo<Option<IslandRequest>>,
    morph: RefCell<IslandMorph>,
    slot: Cell<Option<IslandSlot>>,
}

impl Island {
    /// Where it is going and where it is now, while it shows.
    fn visible(&self) -> Option<(IslandSlot, LayoutRect)> {
        self.slot.get().zip(self.morph.borrow().rect())
    }
}

/// The part of an island that sticks out above the bar, which is all that shapes the
/// silhouette: an island over the bar is the bar's color already.
fn above_bar(rect: LayoutRect) -> LayoutRect {
    let y = rect.y.min(BAR_TOP);
    LayoutRect::new(rect.x, y, rect.width, BAR_TOP - y)
}

/// An island's destination and resting rectangles on a bar `width` wide.
fn island_rects(request: &IslandRequest, width: f32) -> (LayoutRect, LayoutRect) {
    let h = request.height.min(BAR_TOP);
    match request.placement {
        IslandPlacement::Joined { center_x } => {
            let w = request.width.min(width - ISLAND_SIDE_MARGIN * 2.0);
            let center_x = center_x.unwrap_or(width / 2.0);
            (
                LayoutRect::new(center_x - w / 2.0, BAR_TOP - h, w, h),
                LayoutRect::new(center_x, BAR_TOP, 0.0, 0.0),
            )
        }
        IslandPlacement::OverBar => {
            let bottom = BAR_TOP + BAR_HEIGHT;
            let h = request.height.min(bottom);
            let w = request.width.min(width - ISLAND_SIDE_MARGIN * 2.0);
            (
                LayoutRect::new((width - w) / 2.0, bottom - h, w, h),
                LayoutRect::new(width / 2.0, bottom, 0.0, 0.0),
            )
        }
        IslandPlacement::Corner { from_width } => {
            let w = request.width.max(from_width).min(width);
            (
                LayoutRect::new(width - w, BAR_TOP - h, w, h),
                LayoutRect::new(width - from_width, BAR_TOP, from_width, 0.0),
            )
        }
    }
}

/// The outline of `bar` with `island` growing from its top edge. `island`'s bottom must sit
/// on `bar`'s top.
/// `corner` rounds the island's top corners and `fillet` flares its base into the bar; both
/// shrink as the island gets too small to fit them. A side flush with the end of the bar
/// stays square, since that is the edge of the screen.
fn island_path(path: &mut BezPath, bar: LayoutRect, island: LayoutRect, corner: f32, fillet: f32) {
    const FLUSH: f64 = 0.5;
    let f = f64::from;
    let (bar_x, top, bar_right, bar_bottom) =
        (f(bar.x), f(bar.y), f(bar.x + bar.width), f(bar.y + bar.height));
    let (left, right, island_top) = (f(island.x), f(island.x + island.width), f(island.y));
    let half_height = (f(island.height) / 2.0).max(0.0);
    let corner = f(corner).min(half_height).min(f(island.width) / 2.0);
    let left_corner = if left - bar_x < FLUSH { 0.0 } else { corner };
    let right_corner = if bar_right - right < FLUSH { 0.0 } else { corner };
    let fillet = f(fillet).min(half_height);
    let left_fillet = fillet.min((left - bar_x).max(0.0));
    let right_fillet = fillet.min((bar_right - right).max(0.0));

    // A quarter circle from where the path is, around `corner`, to `to`.
    let arc = |p: &mut BezPath, from: (f64, f64), corner: (f64, f64), to: (f64, f64)| {
        let pull = |a: (f64, f64)| (a.0 + (corner.0 - a.0) * ARC, a.1 + (corner.1 - a.1) * ARC);
        p.curve_to(pull(from), pull(to), to);
    };
    path.move_to((bar_x, top));
    path.line_to((left - left_fillet, top));
    arc(path, (left - left_fillet, top), (left, top), (left, top - left_fillet));
    path.line_to((left, island_top + left_corner));
    arc(
        path,
        (left, island_top + left_corner),
        (left, island_top),
        (left + left_corner, island_top),
    );
    path.line_to((right - right_corner, island_top));
    arc(
        path,
        (right - right_corner, island_top),
        (right, island_top),
        (right, island_top + right_corner),
    );
    path.line_to((right, top - right_fillet));
    arc(path, (right, top - right_fillet), (right, top), (right + right_fillet, top));
    path.line_to((bar_right, top));
    path.line_to((bar_right, bar_bottom));
    path.line_to((bar_x, bar_bottom));
    path.close_path();
}

/// How wide `text` is at `size`, for laying text out before it is shown.
pub fn measure(size: f32) -> impl Fn(&str) -> f32 {
    let style = anti_webui::style::Style::default().font_family(FONT).text_size(size);
    move |s| svg::measure_text(s, &style).0
}

pub fn color(c: ui_core::scheme::color::Color) -> Color {
    Color::new(c.r, c.g, c.b, c.a)
}

/// The palette for the md3e components, which read theirs from one global scheme.
pub fn md3e_scheme(t: &ColorTheme) -> anti_webui_md3e::ColorScheme {
    anti_webui_md3e::ColorScheme {
        primary: color(t.primary),
        on_primary: color(t.on_primary),
        primary_container: color(t.primary_container),
        on_primary_container: color(t.on_primary_container),
        secondary: color(t.secondary),
        on_secondary: color(t.on_secondary),
        secondary_container: color(t.secondary_container),
        on_secondary_container: color(t.on_secondary_container),
        tertiary: color(t.tertiary),
        on_tertiary: color(t.on_tertiary),
        tertiary_container: color(t.tertiary_container),
        on_tertiary_container: color(t.on_tertiary_container),
        error: color(t.error),
        on_error: color(t.on_error),
        error_container: color(t.error_container),
        on_error_container: color(t.on_error_container),
        surface: color(t.surface),
        surface_dim: color(t.surface_dim),
        surface_bright: color(t.surface_bright),
        surface_variant: color(t.surface_variant),
        on_surface: color(t.on_surface),
        on_surface_variant: color(t.on_surface_variant),
        inverse_surface: color(t.inverse_surface),
        inverse_on_surface: color(t.inverse_on_surface),
        inverse_primary: color(t.inverse_primary),
        scrim: color(t.scrim),
        surface_container_lowest: color(t.surface_container_lowest),
        surface_container_low: color(t.surface_container_low),
        surface_container: color(t.surface_container),
        surface_container_high: color(t.surface_container_high),
        surface_container_highest: color(t.surface_container_highest),
        outline: color(t.outline),
        outline_variant: color(t.outline_variant),
    }
}

/// A color from the active palette, read reactively.
pub fn palette(state: &State, pick: impl Fn(&ColorTheme) -> ui_core::scheme::color::Color) -> Color {
    state.theme.with(|t| color(pick(t.theme())))
}

/// `target`, moving to each new value as [`Eased::to`] does.
pub fn eased(
    state: &State,
    easing_id: &'static str,
    duration: Duration,
    target: impl Fn() -> f32 + 'static,
) -> Signal<f32> {
    let target = memo(target);
    let value = Eased::new(state, easing_id, duration, untrack(|| target.get()));
    effect(move || value.to(target.get()));
    value.value
}

#[derive(Clone, Copy, PartialEq)]
enum Goal {
    Ease(f32),
    Jump(f32),
}

/// A value that moves over `duration` along `animation.toml`'s `easing_id` (ease-out-cubic
/// until it names one), a frame at a time. For what AntiWebUI's own transitions can't do: they
/// take a curve, not the config's easing functions, so no `linear()` springs.
#[derive(Clone, Copy)]
pub struct Eased {
    value: Signal<f32>,
    goal: Signal<Goal>,
}

impl Eased {
    pub fn new(state: &State, easing_id: &'static str, duration: Duration, start: f32) -> Self {
        fn step(value: Signal<f32>, animation: Rc<RefCell<Animation<f32>>>, ticking: Rc<Cell<bool>>) {
            request_frame(move |_| {
                value.set(animation.borrow().value());
                if animation.borrow().is_traveling() {
                    step(value, animation, ticking);
                } else {
                    ticking.set(false);
                }
            });
        }

        let value = signal(start);
        let goal = signal(Goal::Jump(start));
        let animation = Rc::new(RefCell::new(Animation::new(start, start, duration, std::sync::Arc::new(ease_out_cubic))));
        let ticking = Rc::new(Cell::new(false));
        {
            let (animation, config) = (animation.clone(), state.animation);
            effect(move || {
                if let Some(easing) = config.with(|a| a.easings().get(easing_id).cloned()) {
                    animation.borrow_mut().set_easing(easing);
                }
            });
        }
        effect(move || match goal.get() {
            Goal::Jump(to) => {
                animation.borrow_mut().reset(to, to);
                value.set(to);
            }
            Goal::Ease(to) => {
                animation.borrow_mut().set_target(to);
                if !ticking.replace(true) {
                    step(value, animation.clone(), ticking.clone());
                }
            }
        });
        Self { value, goal }
    }

    pub fn get(self) -> f32 {
        self.value.get()
    }

    pub fn get_untracked(self) -> f32 {
        untrack(|| self.value.get())
    }

    /// Moves to `to` from wherever it is now.
    pub fn to(self, to: f32) {
        self.goal.set(Goal::Ease(to));
    }

    /// Is at `to` at once, as under a dragging pointer.
    pub fn set(self, to: f32) {
        self.goal.set(Goal::Jump(to));
    }
}

/// Repaints every frame for as long as an island moves.
fn tick(frame: Signal<u64>, islands: Rc<[Island]>, ticking: Rc<Cell<bool>>) {
    request_frame(move |_| {
        for island in islands.iter() {
            if island.morph.borrow().rect().is_none() {
                island.slot.set(None);
            }
        }
        frame.update(|f| *f += 1);
        if islands.iter().any(|i| i.morph.borrow().is_traveling()) {
            tick(frame, islands, ticking);
        } else {
            ticking.set(false);
        }
    });
}

/// The bar on a screen `width` wide, BAR_HEIGHT + ISLAND_MAX_HEIGHT tall, with `parts` in
/// it. `input` follows what takes the pointer: the bar, and the island taking it.
pub fn bar(state: &State, width: f32, parts: Vec<Part>, input: Signal<Vec<RectI>>) -> Bar {
    let mut slots: [Vec<View>; 3] = Default::default();
    // Which parts are in each slot, in order, and how wide each is, for where each part is.
    let mut members: [Vec<usize>; 3] = Default::default();
    let widths: Rc<[Signal<f32>]> = parts.iter().map(|_| signal(0.0)).collect();
    let mut islands = Vec::new();
    let mut contents = Vec::new();
    let mut closes = Vec::new();
    let mut frames = Vec::new();
    let mut overlays = Vec::new();
    let mut bar_views = Vec::new();
    for (i, part) in parts.into_iter().enumerate() {
        if let Some((slot, view)) = part.bar {
            members[slot as usize].push(i);
            bar_views.push((slot, i, view));
        }
        if let Some(overlay) = part.overlay {
            overlays.push((i, overlay));
        }
        if let Some(island) = part.island {
            frames.push((i, island.frame));
            islands.push(Island {
                request: island.request,
                morph: RefCell::new(IslandMorph::new(ISLAND_MORPH, std::sync::Arc::new(ease_out_cubic))),
                slot: Cell::new(None),
            });
            contents.push(island.content);
            closes.push((i, island.close));
        }
    }
    let islands: Rc<[Island]> = islands.into();
    // A press on the bar closes the islands of every part but the one pressed, as one would
    // otherwise keep its island over the one the press is opening. The part's own press
    // handlers run first, from deeper in.
    let closes: Rc<[(usize, Rc<dyn Fn()>)]> = closes.into();
    let close_all_but = {
        let closes = closes.clone();
        move |part: Option<usize>| {
            for (owner, close) in closes.iter() {
                if Some(*owner) != part {
                    close();
                }
            }
        }
    };
    for (slot, i, view) in bar_views {
        let width = widths[i];
        let close_others = close_all_but.clone();
        slots[slot as usize].push(
            div()
                .flex_none()
                .on_resized(move |w, _| width.set(w))
                .on_pointer_down(move |e| {
                    e.stop_propagation();
                    close_others(Some(i));
                })
                .child(view)
                .into_view(),
        );
    }
    let origin = {
        let widths = widths.clone();
        Rc::new(move |part: usize| part_x(&members, &widths.iter().map(|w| w.get()).collect::<Vec<_>>(), width, part))
    };
    let frame = signal(0u64);
    let main = signal(None::<usize>);
    let ticking = Rc::new(Cell::new(false));

    // Bids decide the main island; every change sets each island moving.
    {
        let islands = islands.clone();
        effect(move || {
            let requests: Vec<_> = islands.iter().map(|i| i.request.get()).collect();
            let winner = requests
                .iter()
                .enumerate()
                .filter_map(|(i, r)| Some((i, (*r)?)))
                .max_by_key(|(_, r)| r.priority);
            for (i, island) in islands.iter().enumerate() {
                match winner {
                    Some((w, request)) if w == i => {
                        let (target, rest) = island_rects(&request, width);
                        island.morph.borrow_mut().morph_to(target, rest);
                        island.slot.set(Some(IslandSlot {
                            target,
                            placement: request.placement,
                        }));
                    }
                    _ => island.morph.borrow_mut().collapse(),
                }
            }
            untrack(|| main.set(winner.map(|(w, _)| w)));
            if !ticking.replace(true) {
                tick(frame, islands.clone(), ticking.clone());
            }
        });
    }

    {
        let islands = islands.clone();
        let animation = state.animation;
        effect(move || {
            if let Some(easing) = animation.with(|a| a.easings().get(ISLAND_EASING_ID).cloned()) {
                for island in islands.iter() {
                    island.morph.borrow_mut().set_easing(easing.clone());
                }
            }
        });
    }

    // The island taking the pointer: the main one, or one still collapsing without one.
    {
        let islands = islands.clone();
        let last = RefCell::new(Vec::new());
        effect(move || {
            frame.get();
            let taking = main
                .get()
                .or_else(|| islands.iter().position(|i| i.visible().is_some()))
                .and_then(|i| islands[i].visible());
            let mut rects = vec![RectI::new(0, BAR_TOP as i32, width as i32, BAR_HEIGHT as i32)];
            if let Some((_, r)) = taking {
                rects.push(RectI::new(
                    r.x.floor() as i32,
                    r.y.floor() as i32,
                    r.width.ceil() as i32,
                    r.height.ceil() as i32,
                ));
            }
            if *last.borrow() != rects {
                *last.borrow_mut() = rects.clone();
                untrack(|| input.set(rects));
            }
        });
    }

    // Each part's island, from its place in the bar, for what travels between the two.
    {
        let (islands, origin) = (islands.clone(), origin.clone());
        effect(move || {
            frame.get();
            for (island, (part, signal)) in islands.iter().zip(&frames) {
                let next = island.visible().map(|(slot, rect)| IslandFrame {
                    progress: slot.content_transform(rect).2,
                    content: LayoutRect::new(
                        slot.target.x - origin(*part),
                        slot.target.y - BAR_TOP,
                        slot.target.width,
                        slot.target.height,
                    ),
                });
                if signal.with_untracked(|f| *f != next) {
                    untrack(|| signal.set(next));
                }
            }
        });
    }

    let silhouette = {
        let islands = islands.clone();
        let state = state.clone();
        svg().w(width).h(BAR_TOP + BAR_HEIGHT).child(
            svg::path(move || {
                frame.get();
                let bar = LayoutRect::new(0.0, BAR_TOP, width, BAR_HEIGHT);
                let mut path = BezPath::new();
                island_path(&mut path, bar, LayoutRect::new(0.0, BAR_TOP, 0.0, 0.0), 0.0, 0.0);
                for (slot, rect) in islands.iter().filter_map(Island::visible) {
                    let scale = slot.content_transform(rect).2;
                    island_path(
                        &mut path,
                        bar,
                        above_bar(rect),
                        ISLAND_CORNER * scale,
                        ISLAND_FILLET * scale,
                    );
                }
                path
            })
            .dyn_style(move |s| s.fill(palette(&state, |t| t.surface_container))),
        )
    };

    let group = || div().flex().flex_row().items_center().gap(BAR_GAP);
    let [left, center, right] = slots;
    let row = div()
        .absolute()
        .left(0.0)
        .top(BAR_TOP)
        .w(width)
        .h(BAR_HEIGHT)
        .flex()
        .flex_row()
        .items_center()
        .on_pointer_down({
            let close_all = close_all_but.clone();
            move |_| close_all(None)
        })
        .child(group().pl(BAR_PADDING).children(left))
        .child(div().flex_1())
        .child(group().children(right))
        .child(
            group()
                .absolute()
                .left(0.0)
                .top(0.0)
                .w(width)
                .h(BAR_HEIGHT)
                .justify_center()
                .pointer_events_none()
                .child(group().pointer_events_auto().children(center)),
        );

    let island_views = contents.into_iter().enumerate().map(|(i, content)| {
        let islands = islands.clone();
        div()
            .absolute()
            .left(0.0)
            .top(0.0)
            .transform_origin(0.0, 0.0)
            .dyn_style(move |s| {
                frame.get();
                match islands[i].visible() {
                    Some((slot, rect)) => {
                        let (x, y, scale) = slot.content_transform(rect);
                        // The main island goes on top of any collapsing into the same place.
                        let z = if main.get() == Some(i) { 2 } else { 1 };
                        s.translate(x, y)
                            .w(slot.target.width)
                            .h(slot.target.height)
                            .scale(scale)
                            .opacity(scale)
                            .z_index(z)
                    }
                    None => s.hidden(),
                }
            })
            .child(content)
    });

    let view = div()
        .relative()
        .w(width)
        .h(BAR_TOP + BAR_HEIGHT)
        .select_none()
        .font_family("Noto Sans CJK JP")
        // ponytail: drops are only logged, as in the Skia bar; a part takes them when one needs to.
        .on_drop_files(|files, _| log::info!("Dropped files: {files:?}"))
        .on_drop_text(|text, _| log::info!("Dropped text: {text:?}"))
        .child(silhouette)
        .child(row)
        .children(island_views)
        .children(overlays.into_iter().map(|(part, overlay)| {
            let origin = origin.clone();
            div()
                .absolute()
                .top(BAR_TOP)
                .z_index(3)
                .pointer_events_none()
                .dyn_style(move |s| s.left(origin(part)))
                .child(overlay)
        }));
    Bar {
        view,
        open: memo(move || main.get().is_some()),
        close: Rc::new(move || close_all_but(None)),
    }
}

/// Where `part` starts on a bar `width` wide, from which parts are in each [`BarSlot`] and how
/// wide each part is, as the bar's row lays them out.
fn part_x(members: &[Vec<usize>; 3], widths: &[f32], width: f32, part: usize) -> f32 {
    let span = |parts: &[usize]| {
        parts.iter().map(|&p| widths[p]).sum::<f32>() + BAR_GAP * parts.len().saturating_sub(1) as f32
    };
    for (slot, parts) in members.iter().enumerate() {
        let Some(at) = parts.iter().position(|&p| p == part) else {
            continue;
        };
        let before = span(&parts[..at]) + if at > 0 { BAR_GAP } else { 0.0 };
        let start = match slot {
            0 => BAR_PADDING,
            1 => (width - span(parts)) / 2.0,
            _ => width - span(parts),
        };
        return start + before;
    }
    0.0
}

/// `screen` with `holes` cut out of it, as rectangles: row by row between the holes' edges,
/// what in each row no hole covers.
pub fn cut_out(screen: RectI, holes: &[RectI]) -> Vec<RectI> {
    let bottom = |r: &RectI| r.y + r.height;
    let right = |r: &RectI| r.x + r.width;
    let mut ys: Vec<i32> = [screen.y, bottom(&screen)]
        .into_iter()
        .chain(holes.iter().flat_map(|h| [h.y, bottom(h)]))
        .filter(|y| (screen.y..=bottom(&screen)).contains(y))
        .collect();
    ys.sort_unstable();
    ys.dedup();
    let mut rects = Vec::new();
    for band in ys.windows(2) {
        let (top, height) = (band[0], band[1] - band[0]);
        let mut covered: Vec<(i32, i32)> = holes
            .iter()
            .filter(|h| h.y <= top && bottom(h) >= band[1])
            .map(|h| (h.x, right(h)))
            .collect();
        covered.sort_unstable();
        let mut x = screen.x;
        for (from, to) in covered.into_iter().chain([(right(&screen), right(&screen))]) {
            if from > x {
                rects.push(RectI::new(x, top, from.min(right(&screen)) - x, height));
            }
            x = x.max(to);
        }
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_sit_where_the_row_lays_them_out() {
        // Two on the left, one centered, two on the right running to the edge.
        let members = [vec![0, 1], vec![2], vec![3, 4]];
        let widths = [10.0, 20.0, 40.0, 30.0, 50.0];
        let x = |part| part_x(&members, &widths, 1000.0, part);
        assert_eq!(x(0), BAR_PADDING);
        assert_eq!(x(1), BAR_PADDING + 10.0 + BAR_GAP);
        assert_eq!(x(2), 480.0);
        assert_eq!(x(4), 950.0);
        assert_eq!(x(3), 950.0 - BAR_GAP - 30.0);
    }

    #[test]
    fn cut_out_leaves_everything_but_the_bar_and_the_island() {
        let screen = RectI::new(0, 0, 100, 100);
        let bar = RectI::new(0, 90, 100, 10);
        let island = RectI::new(60, 50, 30, 40);
        let rects = cut_out(screen, &[bar, island]);
        assert_eq!(
            rects,
            [RectI::new(0, 0, 100, 50), RectI::new(0, 50, 60, 40), RectI::new(90, 50, 10, 40)]
        );
        assert_eq!(cut_out(screen, &[]), [screen]);
        let area = |r: &[RectI]| r.iter().map(|r| r.width * r.height).sum::<i32>();
        assert_eq!(area(&rects), 100 * 100 - 100 * 10 - 30 * 40);
    }

    fn slot(placement: IslandPlacement) -> IslandSlot {
        IslandSlot {
            target: LayoutRect::new(0.0, 0.0, 400.0, 100.0),
            placement,
        }
    }

    #[test]
    fn joined_content_scales_from_the_bottom_center() {
        let slot = slot(IslandPlacement::Joined { center_x: None });

        let full = LayoutRect::new(200.0, 140.0, 400.0, 100.0);
        assert_eq!(slot.content_transform(full), (200.0, 140.0, 1.0));

        let half = LayoutRect::new(300.0, 190.0, 200.0, 50.0);
        assert_eq!(slot.content_transform(half), (300.0, 190.0, 0.5));

        // Resizing to a shorter, wider island fits the content to the height.
        let wide = LayoutRect::new(100.0, 190.0, 600.0, 50.0);
        assert_eq!(slot.content_transform(wide), (300.0, 190.0, 0.5));
    }

    #[test]
    fn corner_content_scales_from_the_bottom_right() {
        let slot = slot(IslandPlacement::Corner { from_width: 200.0 });

        let full = LayoutRect::new(600.0, 200.0, 400.0, 100.0);
        assert_eq!(slot.content_transform(full), (600.0, 200.0, 1.0));

        let half = LayoutRect::new(700.0, 250.0, 300.0, 50.0);
        assert_eq!(slot.content_transform(half), (800.0, 250.0, 0.5));

        let start = LayoutRect::new(800.0, 300.0, 200.0, 0.0);
        assert_eq!(slot.content_transform(start).2, 0.0);
    }

    #[test]
    fn corner_island_is_flush_with_the_screen_and_rests_on_the_bar() {
        let request = IslandRequest {
            width: 380.0,
            height: 200.0,
            placement: IslandPlacement::Corner { from_width: 150.0 },
            priority: 0,
        };
        let (target, rest) = island_rects(&request, 1000.0);

        assert_eq!(target, LayoutRect::new(620.0, BAR_TOP - 200.0, 380.0, 200.0));
        assert_eq!(rest, LayoutRect::new(850.0, BAR_TOP, 150.0, 0.0));
    }

    #[test]
    fn over_bar_island_reaches_the_screen_edge_and_only_shapes_what_is_above_the_bar() {
        let bottom = BAR_TOP + BAR_HEIGHT;
        let request = IslandRequest {
            width: 300.0,
            height: 200.0,
            placement: IslandPlacement::OverBar,
            priority: 0,
        };
        let (target, rest) = island_rects(&request, 1000.0);
        assert_eq!(target, LayoutRect::new(350.0, bottom - 200.0, 300.0, 200.0));
        assert_eq!(rest, LayoutRect::new(500.0, bottom, 0.0, 0.0));

        assert_eq!(
            above_bar(target),
            LayoutRect::new(350.0, bottom - 200.0, 300.0, 200.0 - BAR_HEIGHT)
        );
        let inside = LayoutRect::new(450.0, BAR_TOP + 10.0, 100.0, 50.0);
        assert_eq!(above_bar(inside), LayoutRect::new(450.0, BAR_TOP, 100.0, 0.0));
    }

    #[test]
    fn island_path_matches_the_bar_when_there_is_no_island() {
        let bar = LayoutRect::new(0.0, BAR_TOP, 800.0, BAR_HEIGHT);
        let mut path = BezPath::new();
        island_path(&mut path, bar, LayoutRect::new(400.0, BAR_TOP, 0.0, 0.0), 28.0, 20.0);
        let bounds = kurbo::Shape::bounding_box(&path);
        assert_eq!(bounds, kurbo::Rect::new(0.0, BAR_TOP as f64, 800.0, (BAR_TOP + BAR_HEIGHT) as f64));
    }

    fn path_of(bar: LayoutRect, island: LayoutRect) -> BezPath {
        let mut path = BezPath::new();
        island_path(&mut path, bar, island, 24.0, 12.0);
        path
    }

    fn contains(path: &BezPath, (x, y): (f64, f64)) -> bool {
        kurbo::Shape::contains(path, kurbo::Point::new(x, y))
    }

    #[test]
    fn island_path_joins_the_island_to_the_bar() {
        let bar = LayoutRect::new(0.0, 100.0, 400.0, 60.0);
        let path = path_of(bar, LayoutRect::new(150.0, 40.0, 100.0, 60.0));
        assert!(contains(&path, (200.0, 45.0)), "island body");
        assert!(contains(&path, (200.0, 130.0)), "bar body");
        assert!(!contains(&path, (151.0, 41.0)), "rounded top corner");
        assert!(contains(&path, (149.0, 99.0)), "fillet flares into the bar");
        assert!(!contains(&path, (140.0, 90.0)), "outside the fillet");
    }

    #[test]
    fn island_path_stays_square_at_the_end_of_the_bar() {
        let bar = LayoutRect::new(0.0, 100.0, 400.0, 60.0);
        let path = path_of(bar, LayoutRect::new(300.0, 40.0, 100.0, 60.0));
        assert!(contains(&path, (399.5, 40.5)), "flush side stays square");
        assert!(!contains(&path, (301.0, 41.0)), "free side is rounded");
        assert!(contains(&path, (299.0, 99.0)), "free side flares into the bar");
        let edge = path_of(bar, LayoutRect::new(0.0, 40.0, 100.0, 60.0));
        assert_eq!(kurbo::Shape::bounding_box(&edge).x0, 0.0);
    }
}
