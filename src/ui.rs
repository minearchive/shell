use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use calloop::channel::Sender;
use macros::boxed;

use skia_safe::{Canvas, Color, Color4f, Font, Paint, Rect};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};
use tokio::sync::mpsc::UnboundedSender;

use mpris::Event as MprisEvent;

use ui_core::{
    animation::{animation::easing::ease_out_cubic, parser::Easing},
    font::FontBook,
    geometry::LayoutRect,
    island::{island_path, IslandMorph},
    scheme::ColorTheme,
};

use crate::{
    components::{clock::Clock, notifications::Notifications, toolbox::Toolbox, warp::Warp},
    config::{animation::AnimationConfig, theme::Theme},
    dbus::{
        kdeconnect::KDEConnectEvent,
        mpris::PlayerState,
        notification::{NotificationEvent, NotificationStore},
        warp::WarpStatus,
    },
    ipc::events::IPCEvent,
    system::{SystemCommand, SystemStatus},
};

pub const BAR_HEIGHT: u32 = 60;
/// Headroom above the bar that islands can grow into. The surface always includes
/// it, transparent and click-through, so growing never waits on a configure.
///
/// ponytail: every repaint fills the full-width headroom too. Resize the surface
/// with the island if that shows up on a profile.
pub const ISLAND_MAX_HEIGHT: u32 = 300;

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
    /// `center_x` of `None` centers it on the bar.
    Joined { center_x: Option<f32> },
    /// Flush with the screen's right edge, widening from `from_width` and scaling its
    /// content from the bottom-right.
    Corner { from_width: f32 },
}

/// A component's bid for the bar's island. Clamped to the available space.
pub struct IslandRequest {
    /// The size of the content passed to `draw_island`.
    pub width: f32,
    pub height: f32,
    pub placement: IslandPlacement,
    /// The highest priority wins. On a tie, the later component does.
    pub priority: u8,
}

/// An island as of this frame, in the coordinates of its owner's bar slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IslandFrame {
    /// How far open it is: 0 at rest, 1 fully open.
    pub progress: f32,
    /// Where its content sits once fully open.
    pub content: LayoutRect,
}

/// Who holds the island, and where it is going.
#[derive(Clone, Copy, Debug)]
struct IslandSlot {
    owner: usize,
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
            IslandPlacement::Joined { .. } => rect.x + (rect.width - width * scale) / 2.0,
            IslandPlacement::Corner { .. } => rect.x + rect.width - width * scale,
        };
        (x, rect.y + rect.height - height * scale, scale)
    }
}

/// Which end of the bar a component sits at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BarSlot {
    /// Takes no space in the bar, such as one that only uses the island.
    #[default]
    None,
    Left,
    Right,
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
    /// Asked before every draw. Return a request for as long as you want the
    /// island.
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
    /// Draws over the island's content, unscaled, with the origin at your bar slot.
    /// `island` is set while you hold the island, so something can move between the
    /// bar and the island.
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
    DismissNotification(u32),
}

/// Provider state shared by every screen, so a screen that appears late sees
/// what the others already know.
#[derive(Default)]
pub struct UIState {
    /// Keyed by bus name: two instances of a player share an identity.
    pub players: HashMap<String, PlayerState>,
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

/// Each component's x in the bar: `Left` slots run in order from the left end,
/// `Right` slots end flush with the screen's edge, and empty ones take no gap.
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
    morph: IslandMorph,
    /// Who the island belongs to.
    island_slot: Option<IslandSlot>,
    /// Where the island was on the last draw.
    island_rect: Option<LayoutRect>,
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
    ) -> Self {
        let components: Vec<Box<dyn Component>> = boxed!(
            Clock,
            Warp,
            Notifications::new(ui_tx),
            Toolbox::new(system_commands)
        );
        let easing = animation
            .read()
            .unwrap()
            .get_easing(ISLAND_EASING_ID)
            .cloned()
            .unwrap_or(Arc::new(ease_out_cubic));

        Self {
            components,
            theme,
            morph: IslandMorph::new(ISLAND_MORPH, easing),
            island_slot: None,
            island_rect: None,
            island_grab: None,
            island_hover: None,
            bar_origins: Vec::new(),
        }
    }

    /// Where the island was on the last draw, in surface coordinates.
    pub fn island_rect(&self) -> Option<LayoutRect> {
        self.island_rect
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
        match request {
            Some((owner, request)) => {
                let (target, rest) = island_rects(&request, width);
                self.morph.morph_to(target, rest);
                self.island_slot = Some(IslandSlot {
                    owner,
                    target,
                    placement: request.placement,
                });
            }
            None => self.morph.collapse(),
        }
        self.island_rect = self.morph.rect();
        if self.island_rect.is_none() {
            self.island_slot = None;
        }
        let visible = self.island_slot.zip(self.island_rect);

        let scale = visible.map_or(0.0, |(slot, rect)| slot.content_transform(rect).2);
        let silhouette = island_path(
            LayoutRect::new(0.0, BAR_TOP, width, BAR_HEIGHT as f32),
            self.island_rect
                .unwrap_or(LayoutRect::new(width / 2.0, BAR_TOP, 0.0, 0.0)),
            ISLAND_CORNER * scale,
            ISLAND_FILLET * scale,
        );
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

        if let Some((slot, rect)) = visible {
            let (x, y, scale) = slot.content_transform(rect);
            let size = (slot.target.width, slot.target.height);
            let saved = canvas.save();
            canvas.translate((x, y));
            canvas.scale((scale, scale));
            canvas.save_layer_alpha_f(Rect::from_wh(size.0, size.1), scale);
            redraw = redraw
                .max(self.components[slot.owner].draw_island(canvas, size, state, fonts, theme));
            canvas.restore_to_count(saved);
        }

        for (i, (c, origin)) in self
            .components
            .iter_mut()
            .zip(&self.bar_origins)
            .enumerate()
        {
            let island = visible
                .filter(|(slot, _)| slot.owner == i)
                .map(|(slot, _)| IslandFrame {
                    progress: scale,
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

        if self.morph.is_traveling() {
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
                self.island_slot
                    .zip(self.island_rect)
                    .filter(|(_, rect)| rect.contains(x as f32, y as f32))
                    .map(|(slot, _)| slot.owner)
            })
        };
        if let Some(owner) = target {
            match event.kind {
                PointerEventKind::Press { .. } => self.island_grab = Some(owner),
                PointerEventKind::Release { .. } => self.island_grab = None,
                _ => {}
            }
            self.island_hover = Some(owner);
            local.position = self.to_island_coords(x, y);
            return self.components[owner].on_island_cursor(&local);
        }

        let mut redraw = Redraw::None;
        self.island_grab = None;
        if let Some(owner) = self.island_hover.take() {
            local.position = self.to_island_coords(x, y);
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

    /// Maps a surface position into the island content's coordinates.
    fn to_island_coords(&self, x: f64, y: f64) -> (f64, f64) {
        match self.island_slot.zip(self.island_rect) {
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
            self.morph.set_easing(easing.clone());
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
    use std::path::Path;
    use std::thread;

    use skia_safe::{surfaces, FontStyle, Surface};

    use super::*;
    use crate::{
        config::WatchableConfig,
        dbus::notification::{NotificationSnapshot, Urgency},
    };

    const SIZE: (i32, i32) = (800, (ISLAND_MAX_HEIGHT + BAR_HEIGHT) as i32);
    const ABOVE_BAR: i32 = BAR_TOP as i32 - 5;
    const IN_BAR: i32 = BAR_TOP as i32 + 50;

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
            owner: 0,
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

    fn alpha(surface: &mut Surface, x: i32, y: i32) -> u8 {
        surface.peek_pixels().unwrap().get_color((x, y)).a()
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
        );
        let mut fonts = FontBook::new();
        fonts.register("noto_sans", "Noto Sans CJK JP", FontStyle::normal());
        let mut state = UIState::default();
        let mut surface = surfaces::raster_n32_premul(SIZE).unwrap();
        let mut draw = |ui: &mut UserInterface, state: &UIState| {
            ui.draw(surface.canvas(), state, &fonts, SIZE.0 as f32);
            // Let the morph finish, then paint its end state.
            thread::sleep(ISLAND_MORPH + Duration::from_millis(20));
            ui.draw(surface.canvas(), state, &fonts, SIZE.0 as f32);
            surface.clone()
        };

        let mut idle = draw(&mut ui, &state);
        assert_eq!(ui.island_rect(), None);
        assert_eq!(
            alpha(&mut idle, 400, ABOVE_BAR),
            0,
            "headroom is transparent"
        );
        assert_eq!(alpha(&mut idle, 10, IN_BAR), 255, "bar is opaque");

        state.notifications.apply(
            &NotificationEvent::Posted {
                id: 1,
                data: NotificationSnapshot {
                    app_name: "Slack".into(),
                    summary: "田中さん".into(),
                    body: "明日のレビューの件について、資料を共有します。".into(),
                    urgency: Urgency::Normal,
                    expire_timeout: 0,
                    ..Default::default()
                },
            },
            std::time::Instant::now(),
        );
        let mut open = draw(&mut ui, &state);
        let rect = ui.island_rect().expect("island is open");
        assert_eq!(rect.y + rect.height, BAR_TOP);
        assert_eq!(rect.x + rect.width / 2.0, 400.0);
        assert_eq!(alpha(&mut open, 400, ABOVE_BAR), 255, "island is opaque");
        assert_eq!(
            alpha(&mut open, 10, ABOVE_BAR),
            0,
            "beside it stays transparent"
        );

        state.notifications.remove(1);
        let mut closed = draw(&mut ui, &state);
        assert_eq!(ui.island_rect(), None);
        assert!(ui.island_slot.is_none());
        assert_eq!(alpha(&mut closed, 400, ABOVE_BAR), 0);
    }
}
