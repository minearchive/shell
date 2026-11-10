use std::{
    collections::HashMap,
    sync::{mpsc, Arc, RwLock},
    time::Duration,
};

use calloop::channel::Sender;
use macros::boxed;

use skia_safe::{Canvas, Color, Color4f, Font, Paint, PathBuilder, Rect};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};
use tokio::sync::mpsc::UnboundedSender;

use mpris::Event as MprisEvent;

use ui_core::{
    animation::{animation::easing::ease_out_cubic, parser::Easing},
    font::FontBook,
    geometry::LayoutRect,
    island::{island_path, IslandMorph},
    pointer::{PointerEvent as CorePointerEvent, PointerEventKind as CoreKind},
    scheme::ColorTheme,
};

use crate::{
    cli::Action,
    components::{clock::Clock, media::Media, toolbox::Toolbox},
    config::{animation::AnimationConfig, theme::Theme},
    dbus::{
        kdeconnect::KDEConnectEvent,
        mpris::{PlayerCommand, PlayerState},
        notification::{NotificationEvent, NotificationStore},
        player_art::Art,
        warp::{WarpCommand, WarpStatus},
    },
    ipc::events::IPCEvent,
    system::{SystemCommand, SystemStatus},
};

pub const BAR_HEIGHT: u32 = 48;
/// Headroom above the bar that islands can grow into. The surface always includes
/// it, transparent and click-through, so growing never waits on a configure.
///
/// ponytail: every repaint fills the full-width headroom too. Resize the surface
/// with the island if that shows up on a profile.
pub const ISLAND_MAX_HEIGHT: u32 = 360;

const BAR_TOP: f32 = ISLAND_MAX_HEIGHT as f32;
const ISLAND_CORNER: f32 = 28.0;
const ISLAND_FILLET: f32 = 20.0;
/// Keeps room beside an island at the bar's edge for its fillet.
pub const ISLAND_SIDE_MARGIN: f32 = 24.0;
const ISLAND_MORPH: Duration = Duration::from_millis(350);
const ISLAND_EASING_ID: &str = "island";
/// Space before the first `Left` slot. `Right` slots run to the screen's edge, so one
/// can attach to it.
const BAR_PADDING: f32 = 20.0;
const BAR_GAP: f32 = 16.0;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum Redraw {
    #[default]
    None,
    Now,
    Animating,
}

/// Where an island rises from the bar's top edge, joined to it by curves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IslandPlacement {
    /// Over a point on the bar, scaling up from nothing at its bottom center.
    /// `center_x` of `None` centers it on the bar. The one docs/components.md starts a
    /// new component from, though no component uses it today.
    #[allow(dead_code)]
    Joined { center_x: Option<f32> },
    /// Centered like `Joined`, but reaching down over the bar to the screen's edge, taking
    /// over the middle of the bar's row for as long as it is open.
    OverBar,
    /// Flush with the screen's right edge, widening from `from_width` and scaling its
    /// content from the bottom-right.
    Corner { from_width: f32 },
}

/// A component's bid for the bar's main island. Clamped to the available space.
pub struct IslandRequest {
    /// The size of the content passed to `draw_island`.
    pub width: f32,
    pub height: f32,
    pub placement: IslandPlacement,
    /// The highest priority wins. On a tie, the later component does.
    pub priority: u8,
}

/// A component's island as of this frame, in the coordinates of its bar slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IslandFrame {
    /// How far open it is: 0 at rest, 1 fully open.
    pub progress: f32,
    /// Where its content sits once fully open.
    pub content: LayoutRect,
}

/// Where an island is going.
#[derive(Clone, Copy, Debug)]
struct IslandSlot {
    target: LayoutRect,
    placement: IslandPlacement,
}

impl IslandSlot {
    /// Where content goes in `rect`: `(x, y, scale)`, scaled uniformly so it never
    /// distorts or overflows, from its bottom-right for a corner island and its bottom
    /// center otherwise. The scale doubles as how far open the island is.
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

/// A component's own island, which opens and collapses where it is, whatever the other
/// components' islands are doing.
struct Island {
    morph: IslandMorph,
    slot: Option<IslandSlot>,
    /// Where it was on the last draw.
    rect: Option<LayoutRect>,
}

impl Island {
    fn visible(&self) -> Option<(IslandSlot, LayoutRect)> {
        self.slot.zip(self.rect)
    }
}

/// Which end of the bar a component sits at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BarSlot {
    /// Takes no space in the bar, such as one that only uses the island.
    #[default]
    None,
    Left,
    /// Centered on the bar, whatever is at either end.
    Center,
    Right,
}

/// A bar pointer event as `m3_widget` wants it. Scrolling has no equivalent.
pub fn to_core_event(event: &PointerEvent) -> Option<CorePointerEvent> {
    let kind = match event.kind {
        PointerEventKind::Enter { .. } => CoreKind::Enter,
        PointerEventKind::Leave { .. } => CoreKind::Leave,
        PointerEventKind::Motion { .. } => CoreKind::Motion,
        PointerEventKind::Press { button, .. } => CoreKind::Press { button },
        PointerEventKind::Release { button, .. } => CoreKind::Release { button },
        PointerEventKind::Axis { .. } => return None,
    };
    Some(CorePointerEvent::new(event.position, kind))
}

/// The baseline that vertically centers `font` in the bar.
pub fn bar_baseline(font: &Font) -> f32 {
    let metrics = font.metrics().1;
    (BAR_HEIGHT as f32 - metrics.descent - metrics.ascent) / 2.0
}

pub trait Component {
    fn bar_slot(&self) -> BarSlot {
        BarSlot::None
    }
    /// How much of the bar this component takes in its slot.
    fn bar_width(&self, _state: &UIState, _fonts: &FontBook) -> f32 {
        0.0
    }
    /// Draws in the bar. The origin is the top-left corner of this component's slot,
    /// which is `BAR_HEIGHT` tall.
    fn draw(
        &mut self,
        _canvas: &Canvas,
        _state: &UIState,
        _fonts: &FontBook,
        _theme: &ColorTheme,
    ) -> Redraw {
        Redraw::None
    }
    /// Asked before every draw. Return a request for as long as you want the main
    /// island. Only the winner's stays open: the others collapse until they win again.
    fn island(
        &mut self,
        _state: &UIState,
        _fonts: &FontBook,
        _bar_width: f32,
    ) -> Option<IslandRequest> {
        None
    }
    /// Draws the island's content at its full `size`, with the origin at its
    /// top-left corner; the island scales and fades it while morphing. Keeps being
    /// called while the island collapses, after `island` has returned `None`, so
    /// hold on to whatever you were showing.
    fn draw_island(
        &mut self,
        _canvas: &Canvas,
        _size: (f32, f32),
        _state: &UIState,
        _fonts: &FontBook,
        _theme: &ColorTheme,
    ) -> Redraw {
        Redraw::None
    }
    /// Draws over the island's content, unscaled, with the origin at your bar slot, and
    /// clipped to the bar and islands so that nothing shows outside them. `island` is set
    /// while your island is visible, so something can move between the bar and the island.
    fn draw_overlay(
        &mut self,
        _canvas: &Canvas,
        _island: Option<IslandFrame>,
        _state: &UIState,
        _fonts: &FontBook,
        _theme: &ColorTheme,
    ) -> Redraw {
        Redraw::None
    }
    /// A pointer event inside your island, relative to its content's origin.
    fn on_island_cursor(&mut self, _events: &PointerEvent) -> Redraw {
        Redraw::None
    }
    /// A pointer event outside the island, relative to this component's slot.
    fn on_cursor(&mut self, _events: &PointerEvent) -> Redraw {
        Redraw::None
    }
    /// Whether to be told about presses outside the bar and island, through
    /// [`Self::on_outside_press`]. Such a press then goes no further: the window under it
    /// doesn't receive it, so only ask for this while something is open to dismiss.
    fn captures_outside_press(&self) -> bool {
        false
    }
    fn on_outside_press(&mut self) -> Redraw {
        Redraw::None
    }
    fn on_ipc(&mut self, _events: &IPCEvent) -> Redraw {
        Redraw::None
    }
    fn on_mpris(&mut self, _state: &PlayerState, _event: &MprisEvent) -> Redraw {
        Redraw::None
    }
    fn on_kde_connect_event(&mut self, _event: &KDEConnectEvent) -> Redraw {
        Redraw::None
    }
    fn on_warp(&mut self, _status: &WarpStatus) -> Redraw {
        Redraw::None
    }
    fn on_action(&mut self, _action: &Action) -> Redraw {
        Redraw::None
    }
    fn on_notification(&mut self, _event: &NotificationEvent) -> Redraw {
        Redraw::None
    }
    fn on_easing_updated(&mut self, _id: &str, _easing: &Easing) -> Redraw {
        Redraw::None
    }
    fn on_drag_enter(&mut self, _mime_types: &[String]) -> Redraw {
        Redraw::None
    }
    fn on_drag_motion(&mut self, _x: f64, _y: f64) -> Redraw {
        Redraw::None
    }
    fn on_drag_leave(&mut self) -> Redraw {
        Redraw::None
    }
    fn on_drop(&mut self, _mime: &str, _data: &[u8]) -> Redraw {
        Redraw::None
    }
}

pub enum UiEvent {
    RequestRedrawAll,
    AnimationUpdated(HashMap<String, Easing>),
    /// Dismisses the notification on display or among the missed ones.
    DismissNotification(u32),
    /// Dismisses every notification, missed ones included.
    ClearNotifications,
}

/// Provider state shared by every screen, so a screen that appears late sees
/// what the others already know.
#[derive(Default)]
pub struct UIState {
    /// Keyed by bus name: two instances of a player share an identity.
    pub players: HashMap<String, PlayerState>,
    /// Keyed by bus name, like `players`.
    pub player_art: HashMap<String, Art>,
    pub warp: Option<WarpStatus>,
    pub notifications: NotificationStore,
    pub system: SystemStatus,
}

impl UIState {
    pub fn update_player(&mut self, state: PlayerState) {
        if state.active {
            self.players.insert(state.bus_name.clone(), state);
        } else {
            self.players.remove(&state.bus_name);
        }
    }
}

/// Each component's x in the bar: `Left` slots run in order from the left end, `Center`
/// slots are centered as a group, `Right` slots end flush with the screen's edge, and
/// empty ones take no gap.
fn bar_origins(slots: &[(BarSlot, f32)], bar_width: f32) -> Vec<f32> {
    let mut origins = vec![0.0; slots.len()];
    let mut x = BAR_PADDING;
    for (origin, (_, width)) in origins
        .iter_mut()
        .zip(slots)
        .filter(|(_, (slot, width))| *slot == BarSlot::Left && *width > 0.0)
    {
        *origin = x;
        x += width + BAR_GAP;
    }
    let centered = slots
        .iter()
        .filter(|(slot, width)| *slot == BarSlot::Center && *width > 0.0);
    let (count, total) = centered.fold((0.0, 0.0), |(count, total), (_, width)| {
        (count + 1.0, total + width)
    });
    let mut x = (bar_width - total - BAR_GAP * (count - 1.0_f32).max(0.0)) / 2.0;
    for (origin, (_, width)) in origins
        .iter_mut()
        .zip(slots)
        .filter(|(_, (slot, width))| *slot == BarSlot::Center && *width > 0.0)
    {
        *origin = x;
        x += width + BAR_GAP;
    }
    let mut x = bar_width;
    for (origin, (_, width)) in origins
        .iter_mut()
        .zip(slots)
        .rev()
        .filter(|(_, (slot, width))| *slot == BarSlot::Right && *width > 0.0)
    {
        x -= width;
        *origin = x;
        x -= BAR_GAP;
    }
    origins
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
            let bottom = BAR_TOP + BAR_HEIGHT as f32;
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

pub struct UserInterface {
    components: Vec<Box<dyn Component>>,
    theme: Arc<RwLock<Theme>>,
    /// Each component's island.
    islands: Vec<Island>,
    /// The component whose island is open, the only one that stays that way.
    main_island: Option<usize>,
    /// The component holding a press that began in its island, which keeps
    /// receiving the pointer until release so a drag can leave the island.
    island_grab: Option<usize>,
    /// The component whose island the pointer is over.
    island_hover: Option<usize>,
    /// Each component's x in the bar on the last draw.
    bar_origins: Vec<f32>,
}

impl UserInterface {
    pub fn new(
        theme: Arc<RwLock<Theme>>,
        animation: &RwLock<AnimationConfig>,
        ui_tx: Sender<UiEvent>,
        system_commands: UnboundedSender<SystemCommand>,
        warp_commands: UnboundedSender<WarpCommand>,
        player_commands: mpsc::Sender<PlayerCommand>,
    ) -> Self {
        let components: Vec<Box<dyn Component>> = boxed!(
            Clock,
            Media::new(player_commands),
            Toolbox::new(system_commands, warp_commands, ui_tx)
        );
        let islands = components
            .iter()
            .map(|_| Island {
                morph: IslandMorph::new(ISLAND_MORPH, Arc::new(ease_out_cubic)),
                slot: None,
                rect: None,
            })
            .collect();

        let mut ui = Self {
            components,
            theme,
            islands,
            main_island: None,
            island_grab: None,
            island_hover: None,
            bar_origins: Vec::new(),
        };
        // Reloads arrive as `on_easing_updated`; the file as it stands at startup does too.
        for (id, easing) in animation.read().unwrap().easings() {
            ui.on_easing_updated(id, easing);
        }
        ui
    }

    /// The island taking the pointer: the main one, or one still collapsing without one.
    fn input_island(&self) -> Option<usize> {
        self.main_island
            .or_else(|| self.islands.iter().position(|i| i.rect.is_some()))
    }

    /// Where the island taking the pointer was on the last draw, in surface coordinates.
    pub fn island_rect(&self) -> Option<LayoutRect> {
        self.islands[self.input_island()?].rect
    }

    fn fan_out(&mut self, f: impl FnMut(&mut Box<dyn Component>) -> Redraw) -> Redraw {
        self.components.iter_mut().map(f).max().unwrap_or_default()
    }

    pub fn captures_outside_press(&self) -> bool {
        self.components.iter().any(|c| c.captures_outside_press())
    }

    pub fn on_outside_press(&mut self) -> Redraw {
        self.fan_out(|c| c.on_outside_press())
    }

    pub fn on_ipc(&mut self, event: &IPCEvent) -> Redraw {
        self.fan_out(|c| c.on_ipc(event))
    }

    pub fn on_mpris(&mut self, state: &PlayerState, event: &MprisEvent) -> Redraw {
        self.fan_out(|c| c.on_mpris(state, event))
    }

    pub fn on_kde_connect_event(&mut self, event: &KDEConnectEvent) -> Redraw {
        self.fan_out(|c| c.on_kde_connect_event(event))
    }

    pub fn on_warp(&mut self, status: &WarpStatus) -> Redraw {
        self.fan_out(|c| c.on_warp(status))
    }

    pub fn on_action(&mut self, action: &Action) -> Redraw {
        self.fan_out(|c| c.on_action(action))
    }

    pub fn on_notification(&mut self, event: &NotificationEvent) -> Redraw {
        self.fan_out(|c| c.on_notification(event))
    }

    pub fn draw(
        &mut self,
        canvas: &Canvas,
        state: &UIState,
        fonts: &FontBook,
        width: f32,
    ) -> Redraw {
        let cfg = self.theme.read().unwrap();
        let theme = cfg.theme();
        canvas.clear(Color::TRANSPARENT);

        let request = self
            .components
            .iter_mut()
            .enumerate()
            .filter_map(|(i, c)| Some((i, c.island(state, fonts, width)?)))
            .max_by_key(|(_, request)| request.priority);
        self.main_island = request.as_ref().map(|(owner, _)| *owner);
        if let Some((owner, request)) = request {
            let (target, rest) = island_rects(&request, width);
            let island = &mut self.islands[owner];
            island.morph.morph_to(target, rest);
            island.slot = Some(IslandSlot {
                target,
                placement: request.placement,
            });
        }
        for (i, island) in self.islands.iter_mut().enumerate() {
            if self.main_island != Some(i) {
                island.morph.collapse();
            }
            island.rect = island.morph.rect();
            if island.rect.is_none() {
                island.slot = None;
            }
        }

        let bar = LayoutRect::new(0.0, BAR_TOP, width, BAR_HEIGHT as f32);
        let flat = LayoutRect::new(width / 2.0, BAR_TOP, 0.0, 0.0);
        let mut silhouette = PathBuilder::new();
        silhouette.add_path(&island_path(bar, flat, 0.0, 0.0), None);
        for (slot, rect) in self.islands.iter().filter_map(Island::visible) {
            let scale = slot.content_transform(rect).2;
            silhouette.add_path(
                &island_path(
                    bar,
                    above_bar(rect),
                    ISLAND_CORNER * scale,
                    ISLAND_FILLET * scale,
                ),
                None,
            );
        }
        let silhouette = silhouette.detach();
        let mut paint = Paint::new(Color4f::from(theme.surface_container), None);
        paint.set_anti_alias(true);
        canvas.draw_path(&silhouette, &paint);

        let slots: Vec<_> = self
            .components
            .iter()
            .map(|c| (c.bar_slot(), c.bar_width(state, fonts)))
            .collect();
        self.bar_origins = bar_origins(&slots, width);

        let mut redraw = Redraw::None;
        for (c, origin) in self.components.iter_mut().zip(&self.bar_origins) {
            canvas.save();
            canvas.translate((*origin, BAR_TOP));
            redraw = redraw.max(c.draw(canvas, state, fonts, theme));
            canvas.restore();
        }

        // The main island goes last, on top of any collapsing into the same place.
        let others = (0..self.islands.len()).filter(|i| self.main_island != Some(*i));
        for i in others.chain(self.main_island).collect::<Vec<_>>() {
            let Some((slot, rect)) = self.islands[i].visible() else {
                continue;
            };
            let (x, y, scale) = slot.content_transform(rect);
            let size = (slot.target.width, slot.target.height);
            let saved = canvas.save();
            canvas.translate((x, y));
            canvas.scale((scale, scale));
            canvas.save_layer_alpha_f(Rect::from_wh(size.0, size.1), scale);
            redraw = redraw.max(self.components[i].draw_island(canvas, size, state, fonts, theme));
            canvas.restore_to_count(saved);
        }

        let saved = canvas.save();
        canvas.clip_path(&silhouette, None, true);
        for ((c, origin), island) in self
            .components
            .iter_mut()
            .zip(&self.bar_origins)
            .zip(&self.islands)
        {
            let island = island.visible().map(|(slot, rect)| IslandFrame {
                progress: slot.content_transform(rect).2,
                content: LayoutRect {
                    x: slot.target.x - origin,
                    y: slot.target.y - BAR_TOP,
                    ..slot.target
                },
            });
            canvas.save();
            canvas.translate((*origin, BAR_TOP));
            redraw = redraw.max(c.draw_overlay(canvas, island, state, fonts, theme));
            canvas.restore();
        }
        canvas.restore_to_count(saved);

        if self.islands.iter().any(|i| i.morph.is_traveling()) {
            redraw = Redraw::Animating;
        }
        redraw
    }

    pub fn on_cursor(&mut self, event: &PointerEvent) -> Redraw {
        let (x, y) = event.position;
        let mut local = event.clone();

        let leaving = matches!(event.kind, PointerEventKind::Leave { .. });
        let target = if leaving {
            None
        } else {
            self.island_grab.or_else(|| {
                self.input_island().filter(|i| {
                    self.islands[*i]
                        .rect
                        .is_some_and(|rect| rect.contains(x as f32, y as f32))
                })
            })
        };
        if let Some(owner) = target {
            match event.kind {
                PointerEventKind::Press { .. } => self.island_grab = Some(owner),
                PointerEventKind::Release { .. } => self.island_grab = None,
                _ => {}
            }
            self.island_hover = Some(owner);
            local.position = self.to_island_coords(owner, x, y);
            return self.components[owner].on_island_cursor(&local);
        }

        let mut redraw = Redraw::None;
        self.island_grab = None;
        if let Some(owner) = self.island_hover.take() {
            local.position = self.to_island_coords(owner, x, y);
            local.kind = PointerEventKind::Leave { serial: 0 };
            redraw = self.components[owner].on_island_cursor(&local);
            local.kind = event.kind.clone();
        }
        for (c, origin) in self.components.iter_mut().zip(&self.bar_origins) {
            local.position = (x - *origin as f64, y - BAR_TOP as f64);
            redraw = redraw.max(c.on_cursor(&local));
        }
        redraw
    }

    /// Maps a surface position into the coordinates of `owner`'s island content.
    fn to_island_coords(&self, owner: usize, x: f64, y: f64) -> (f64, f64) {
        match self.islands[owner].visible() {
            Some((slot, rect)) => {
                let (origin_x, origin_y, scale) = slot.content_transform(rect);
                let scale = f64::from(scale.max(f32::EPSILON));
                (
                    (x - f64::from(origin_x)) / scale,
                    (y - f64::from(origin_y)) / scale,
                )
            }
            None => (x, y),
        }
    }

    pub fn on_easing_updated(&mut self, id: &str, easing: &Easing) -> Redraw {
        if id == ISLAND_EASING_ID {
            for island in &mut self.islands {
                island.morph.set_easing(easing.clone());
            }
        }
        self.fan_out(|c| c.on_easing_updated(id, easing))
    }

    pub fn on_drag_enter(&mut self, mime_types: &[String]) -> Redraw {
        self.fan_out(|c| c.on_drag_enter(mime_types))
    }

    pub fn on_drag_motion(&mut self, x: f64, y: f64) -> Redraw {
        self.fan_out(|c| c.on_drag_motion(x, y - BAR_TOP as f64))
    }

    pub fn on_drag_leave(&mut self) -> Redraw {
        self.fan_out(|c| c.on_drag_leave())
    }

    pub fn on_drop(&mut self, mime: &str, data: &[u8]) -> Redraw {
        self.fan_out(|c| c.on_drop(mime, data))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::Path;
    use std::rc::Rc;
    use std::thread;

    use skia_safe::{surfaces, Surface};

    use super::*;
    use crate::config::WatchableConfig;

    const SIZE: (i32, i32) = (800, (ISLAND_MAX_HEIGHT + BAR_HEIGHT) as i32);
    const ABOVE_BAR: i32 = BAR_TOP as i32 - 5;
    const IN_BAR: i32 = BAR_TOP as i32 + BAR_HEIGHT as i32 / 2;

    #[test]
    fn center_slots_stay_centered_on_the_bar() {
        let slots = [
            (BarSlot::Left, 300.0),
            (BarSlot::Center, 100.0),
            (BarSlot::Center, 0.0),
            (BarSlot::Right, 50.0),
        ];
        assert_eq!(bar_origins(&slots, 1000.0)[1], 450.0);

        let slots = [(BarSlot::Center, 100.0), (BarSlot::Center, 200.0)];
        let origins = bar_origins(&slots, 1000.0);
        assert_eq!(origins[0], (1000.0 - 300.0 - BAR_GAP) / 2.0);
        assert_eq!(origins[1], origins[0] + 100.0 + BAR_GAP);
    }

    #[test]
    fn bar_origins_pack_each_end() {
        let slots = [
            (BarSlot::Left, 100.0),
            (BarSlot::Left, 0.0),
            (BarSlot::None, 50.0),
            (BarSlot::Left, 40.0),
            (BarSlot::Right, 30.0),
            (BarSlot::Right, 60.0),
        ];
        let origins = bar_origins(&slots, 1000.0);

        assert_eq!(origins[0], BAR_PADDING);
        assert_eq!(origins[3], BAR_PADDING + 100.0 + BAR_GAP);
        assert_eq!(origins[5], 1000.0 - 60.0);
        assert_eq!(origins[4], origins[5] - BAR_GAP - 30.0);
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

        assert_eq!(
            target,
            LayoutRect::new(620.0, BAR_TOP - 200.0, 380.0, 200.0)
        );
        assert_eq!(rest, LayoutRect::new(850.0, BAR_TOP, 150.0, 0.0));
    }

    #[test]
    fn over_bar_island_reaches_the_screen_edge_and_only_shapes_what_is_above_the_bar() {
        let bottom = BAR_TOP + BAR_HEIGHT as f32;
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
            LayoutRect::new(350.0, bottom - 200.0, 300.0, 200.0 - BAR_HEIGHT as f32)
        );
        let inside = LayoutRect::new(450.0, BAR_TOP + 10.0, 100.0, 50.0);
        assert_eq!(
            above_bar(inside),
            LayoutRect::new(450.0, BAR_TOP, 100.0, 0.0)
        );
    }

    fn alpha(surface: &mut Surface, x: i32, y: i32) -> u8 {
        surface.peek_pixels().unwrap().get_color((x, y)).a()
    }

    /// Bids for the island at `center_x` when told to, and tries to paint the whole
    /// headroom from the overlay.
    struct Bidder {
        center_x: f32,
        bidding: Rc<Cell<bool>>,
        frame: Rc<Cell<Option<IslandFrame>>>,
    }

    impl Component for Bidder {
        fn island(&mut self, _: &UIState, _: &FontBook, _: f32) -> Option<IslandRequest> {
            self.bidding.get().then_some(IslandRequest {
                width: 200.0,
                height: 200.0,
                placement: IslandPlacement::Joined {
                    center_x: Some(self.center_x),
                },
                priority: 0,
            })
        }

        fn draw_overlay(
            &mut self,
            canvas: &Canvas,
            island: Option<IslandFrame>,
            _: &UIState,
            _: &FontBook,
            _: &ColorTheme,
        ) -> Redraw {
            self.frame.set(island);
            canvas.draw_rect(
                Rect::from_xywh(0.0, -BAR_TOP, SIZE.0 as f32, BAR_TOP),
                &Paint::new(Color4f::new(1.0, 0.0, 0.0, 1.0), None),
            );
            Redraw::None
        }
    }

    #[test]
    fn a_new_main_island_opens_while_the_old_one_collapses_in_its_own_place() {
        const A: i32 = 150;
        const B: i32 = 650;
        let theme = Theme::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("example/theme.toml"));
        let (tx, _rx) = calloop::channel::channel();
        let mut ui = UserInterface::new(
            Arc::new(RwLock::new(theme)),
            &RwLock::new(AnimationConfig::default()),
            tx,
            tokio::sync::mpsc::unbounded_channel().0,
            tokio::sync::mpsc::unbounded_channel().0,
            mpsc::channel().0,
        );
        let bidder = |center_x: i32| Bidder {
            center_x: center_x as f32,
            bidding: Rc::default(),
            frame: Rc::default(),
        };
        let (a, b) = (bidder(A), bidder(B));
        let (a_bids, a_frame, b_bids, b_frame) = (
            a.bidding.clone(),
            a.frame.clone(),
            b.bidding.clone(),
            b.frame.clone(),
        );
        ui.components = vec![Box::new(a), Box::new(b)];
        ui.islands.truncate(2);

        let fonts = FontBook::new();
        let state = UIState::default();
        let mut surface = surfaces::raster_n32_premul(SIZE).unwrap();
        let settle = ISLAND_MORPH + Duration::from_millis(20);

        a_bids.set(true);
        ui.draw(surface.canvas(), &state, &fonts, SIZE.0 as f32);
        thread::sleep(settle);
        ui.draw(surface.canvas(), &state, &fonts, SIZE.0 as f32);
        assert_eq!(a_frame.get().unwrap().progress, 1.0);
        assert_eq!(b_frame.get(), None);
        assert_ne!(alpha(&mut surface, A, ABOVE_BAR), 0);
        assert_eq!(
            alpha(&mut surface, 400, ABOVE_BAR),
            0,
            "the overlay doesn't show outside the island"
        );

        a_bids.set(false);
        b_bids.set(true);
        ui.draw(surface.canvas(), &state, &fonts, SIZE.0 as f32);
        assert!(a_frame.get().unwrap().progress > 0.5, "a is still open");
        let started = b_frame.get().map_or(0.0, |frame| frame.progress);
        assert!(started < 0.5, "b opens from nothing: {started}");
        let input = ui.island_rect().unwrap();
        assert!(
            (input.x + input.width / 2.0 - B as f32).abs() < 1.0,
            "b takes input"
        );

        thread::sleep(ISLAND_MORPH / 3);
        ui.draw(surface.canvas(), &state, &fonts, SIZE.0 as f32);
        assert_ne!(alpha(&mut surface, A, ABOVE_BAR), 0, "both show at once");
        assert_ne!(alpha(&mut surface, B, ABOVE_BAR), 0, "both show at once");
        assert_eq!(alpha(&mut surface, 400, ABOVE_BAR), 0, "as two islands");

        thread::sleep(settle);
        ui.draw(surface.canvas(), &state, &fonts, SIZE.0 as f32);
        assert_eq!(a_frame.get(), None);
        assert_eq!(b_frame.get().unwrap().progress, 1.0);
        assert_eq!(alpha(&mut surface, A, ABOVE_BAR), 0, "only one stays open");
    }

    #[test]
    fn island_grows_from_the_bar_and_collapses_away() {
        let theme = Theme::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("example/theme.toml"));
        let (tx, _rx) = calloop::channel::channel();
        let mut ui = UserInterface::new(
            Arc::new(RwLock::new(theme)),
            &RwLock::new(AnimationConfig::default()),
            tx,
            tokio::sync::mpsc::unbounded_channel().0,
            tokio::sync::mpsc::unbounded_channel().0,
            mpsc::channel().0,
        );
        let bidder = Bidder {
            center_x: 400.0,
            bidding: Rc::default(),
            frame: Rc::default(),
        };
        let bidding = bidder.bidding.clone();
        ui.components = vec![Box::new(bidder)];
        ui.islands.truncate(1);

        let fonts = FontBook::new();
        let state = UIState::default();
        let mut surface = surfaces::raster_n32_premul(SIZE).unwrap();
        let mut draw = |ui: &mut UserInterface| {
            ui.draw(surface.canvas(), &state, &fonts, SIZE.0 as f32);
            // Let the morph finish, then paint its end state.
            thread::sleep(ISLAND_MORPH + Duration::from_millis(20));
            ui.draw(surface.canvas(), &state, &fonts, SIZE.0 as f32);
            surface.clone()
        };

        let mut idle = draw(&mut ui);
        assert_eq!(ui.island_rect(), None);
        assert_eq!(
            alpha(&mut idle, 400, ABOVE_BAR),
            0,
            "headroom is transparent"
        );
        assert_eq!(alpha(&mut idle, 10, IN_BAR), 255, "bar is opaque");

        bidding.set(true);
        let mut open = draw(&mut ui);
        let rect = ui.island_rect().expect("island is open");
        assert_eq!(rect.y + rect.height, BAR_TOP);
        assert_eq!(rect.x + rect.width / 2.0, 400.0);
        assert_eq!(alpha(&mut open, 400, ABOVE_BAR), 255, "island is opaque");
        assert_eq!(
            alpha(&mut open, 10, ABOVE_BAR),
            0,
            "beside it stays transparent"
        );

        bidding.set(false);
        let mut closed = draw(&mut ui);
        assert_eq!(ui.island_rect(), None);
        assert!(ui.islands.iter().all(|island| island.slot.is_none()));
        assert_eq!(alpha(&mut closed, 400, ABOVE_BAR), 0);
    }
}
