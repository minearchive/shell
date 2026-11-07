//! The media player in the middle of the bar: an icon and the title, which open an
//! island with the track and its controls.

use std::{
    collections::HashMap,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};

use m3_widget::{icon::BOX_SIZE, Icon, Slider, Widget};
use mpris::{Event, PlaybackStatus};
use skia_safe::{Canvas, Color4f, Paint, Point, Rect};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};

use ui_core::{
    font::FontBook,
    geometry::LayoutRect,
    pointer::{button, PointerEvent as CorePointerEvent, PointerEventKind as CoreKind},
    scheme::ColorTheme,
    util::{elide, wrap_text},
};

use crate::{
    components::toolbox::lerp_box,
    dbus::mpris::{PlayerAction, PlayerCommand, PlayerState},
    ui::{
        bar_baseline, to_core_event, BarSlot, Component, IslandFrame, IslandPlacement,
        IslandRequest, Redraw, UIState, BAR_HEIGHT, ISLAND_SIDE_MARGIN,
    },
};

const FONT: &str = "noto_sans";
const BAR_TEXT_SIZE: f32 = 16.0;
const MAX_BAR_TEXT_WIDTH: f32 = 280.0;
const ICON_GAP: f32 = 8.0;

const ISLAND_WIDTH: f32 = 360.0;
const PADDING: f32 = 18.0;
/// Less than the sides: the button row has room of its own above the screen's edge.
const BOTTOM_PADDING: f32 = 6.0;
const TITLE_SIZE: f32 = 16.0;
const ARTIST_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 1.4;
const MAX_TITLE_LINES: usize = 2;
const TIME_SIZE: f32 = 12.0;
const TIME_WIDTH: f32 = 44.0;
const SLIDER_ROW_HEIGHT: f32 = 36.0;
const SLIDER_ROW_GAP: f32 = 4.0;
const BUTTON_WIDTH: f32 = 56.0;
const BUTTON_ROW_HEIGHT: f32 = 44.0;
/// Below notifications, which take the island over and then hand it back.
const ISLAND_PRIORITY: u8 = 50;
const BUTTONS: [PlayerAction; 3] = [
    PlayerAction::Previous,
    PlayerAction::PlayPause,
    PlayerAction::Next,
];

/// The player to show, if any: playing beats paused, then whichever last played. A
/// stopped player has nothing to show. `last_playing` keeps the bar on the player the
/// user just paused, rather than jumping to another that happens to be paused too.
fn current_player<'a>(
    players: &'a HashMap<String, PlayerState>,
    last_playing: Option<&str>,
) -> Option<&'a PlayerState> {
    players
        .values()
        .filter(|player| player.status != PlaybackStatus::Stopped)
        .min_by_key(|player| {
            (
                player.status != PlaybackStatus::Playing,
                Some(player.bus_name.as_str()) != last_playing,
                &player.bus_name,
            )
        })
}

/// Falls back on the player's name for a track with no title.
fn title(player: &PlayerState) -> &str {
    player
        .title
        .as_deref()
        .filter(|title| !title.is_empty())
        .unwrap_or(&player.identity)
}

fn bar_title(player: &PlayerState, fonts: &FontBook) -> String {
    elide(title(player), MAX_BAR_TEXT_WIDTH, |s| {
        fonts.measure(FONT, BAR_TEXT_SIZE, s)
    })
}

fn format_time(time: Duration) -> String {
    let seconds = time.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Where text in the island is while it opens: `to` rises with the island's content, and
/// stays at `from` in the bar until the content has come up that far. `from` and `to` are
/// in slot coordinates, as is `content`, the island's final rect.
fn travel(from: Point, to: Point, content: LayoutRect, progress: f32) -> Point {
    let bottom = content.y + content.height;
    Point::new(
        from.x + (to.x - from.x) * progress,
        from.y.min(bottom - (bottom - to.y) * progress),
    )
}

/// The track laid out for the island. Kept after the island closes, so it is still there
/// to draw while it collapses.
struct Shown {
    bus_name: String,
    title: Vec<String>,
    artist: String,
    playing: bool,
    /// `(position, length)`, if the player reports them.
    progress: Option<(Duration, Duration)>,
    size: (f32, f32),
}

impl Shown {
    fn new(player: &PlayerState, bar_width: f32, fonts: &FontBook) -> Self {
        let width = ISLAND_WIDTH.min(bar_width - ISLAND_SIDE_MARGIN * 2.0);
        let text_width = width - PADDING * 2.0;
        let artists = player.artists.as_deref().unwrap_or_default().join(", ");
        let artist = elide(&artists, text_width, |s| {
            fonts.measure(FONT, ARTIST_SIZE, s)
        });
        let title = wrap_text(title(player), text_width, MAX_TITLE_LINES, |s| {
            fonts.measure(FONT, TITLE_SIZE, s)
        });
        let artist_height = if artist.is_empty() {
            0.0
        } else {
            ARTIST_SIZE * LINE_HEIGHT
        };
        let height = PADDING
            + TITLE_SIZE * LINE_HEIGHT * title.len() as f32
            + artist_height
            + SLIDER_ROW_GAP
            + SLIDER_ROW_HEIGHT
            + BUTTON_ROW_HEIGHT
            + BOTTOM_PADDING;
        Self {
            bus_name: player.bus_name.clone(),
            title,
            artist,
            playing: player.status == PlaybackStatus::Playing,
            progress: player.progress(Instant::now()),
            size: (width, height),
        }
    }

    fn slider_rect(&self) -> LayoutRect {
        let (_, top) = self.button_origin();
        let x = PADDING + TIME_WIDTH;
        LayoutRect::new(
            x,
            top - SLIDER_ROW_HEIGHT,
            self.size.0 - x * 2.0,
            SLIDER_ROW_HEIGHT,
        )
    }

    /// The title's and artist's lines: their text, size and baseline's start.
    fn lines(&self, fonts: &FontBook) -> Vec<(&str, f32, Point)> {
        let titles = self.title.iter().map(|line| (line.as_str(), TITLE_SIZE));
        let mut y = PADDING;
        titles
            .chain([(self.artist.as_str(), ARTIST_SIZE)])
            .filter(|(text, _)| !text.is_empty())
            .map(|(text, size)| {
                let x = (self.size.0 - fonts.measure(FONT, size, text)) / 2.0;
                let at = Point::new(x, y + size);
                y += size * LINE_HEIGHT;
                (text, size, at)
            })
            .collect()
    }

    /// The icon of the `index`th button.
    fn button_box(&self, index: usize) -> Rect {
        let (left, top) = self.button_origin();
        Rect::from_xywh(
            left + index as f32 * BUTTON_WIDTH + (BUTTON_WIDTH - BOX_SIZE) / 2.0,
            top + (BUTTON_ROW_HEIGHT - BOX_SIZE) / 2.0,
            BOX_SIZE,
            BOX_SIZE,
        )
    }

    /// The left edge of the first button and the top of the row.
    fn button_origin(&self) -> (f32, f32) {
        (
            (self.size.0 - BUTTON_WIDTH * BUTTONS.len() as f32) / 2.0,
            self.size.1 - BOTTOM_PADDING - BUTTON_ROW_HEIGHT,
        )
    }

    fn button_at(&self, x: f64, y: f64) -> Option<PlayerAction> {
        let (left, top) = self.button_origin();
        let (x, y) = (x as f32 - left, y as f32 - top);
        if x < 0.0 || !(0.0..BUTTON_ROW_HEIGHT).contains(&y) {
            return None;
        }
        BUTTONS.get((x / BUTTON_WIDTH) as usize).copied()
    }
}

pub struct Media {
    commands: Sender<PlayerCommand>,
    last_playing: Option<String>,
    open: bool,
    shown: Option<Shown>,
    slider: Slider,
    /// The width in the bar on the last draw, for hit testing.
    bar_width: f32,
}

impl Media {
    pub fn new(commands: Sender<PlayerCommand>) -> Self {
        Self {
            commands,
            last_playing: None,
            open: false,
            shown: None,
            slider: Slider::new(0.0, 1.0, 0.0),
            bar_width: 0.0,
        }
    }

    fn player<'a>(&self, state: &'a UIState) -> Option<&'a PlayerState> {
        current_player(&state.players, self.last_playing.as_deref())
    }

    fn close(&mut self) -> Redraw {
        if self.open {
            self.open = false;
            Redraw::Now
        } else {
            Redraw::None
        }
    }

    fn send(&self, bus_name: &str, action: PlayerAction) {
        let _ = self.commands.send(PlayerCommand {
            bus_name: bus_name.to_string(),
            action,
        });
    }

    /// A pointer event in the island. The slider only seeks once it is let go, since a
    /// player may take a moment over every seek.
    fn on_island_pointer(&mut self, event: &CorePointerEvent) -> Redraw {
        let Some(shown) = &self.shown else {
            return Redraw::None;
        };
        let was_pressed = self.slider.is_pressed();
        let dirty = self.slider.on_pointer(event);

        match event.kind {
            CoreKind::Release { .. } if was_pressed => {
                if let Some((_, length)) = shown.progress {
                    let target = length.mul_f32(self.slider.value());
                    self.send(&shown.bus_name, PlayerAction::SeekTo(target));
                }
            }
            CoreKind::Press { button } if button == button::LEFT => {
                if let Some(action) = shown.button_at(event.x(), event.y()) {
                    self.send(&shown.bus_name, action);
                }
            }
            _ => {}
        }
        if dirty {
            Redraw::Now
        } else {
            Redraw::None
        }
    }

    /// Our slot toggles the island, and anywhere else in the bar dismisses it. `x` and
    /// `y` are relative to the slot, so the rest of the bar is out of its range.
    fn on_bar_press(&mut self, x: f64, y: f64) -> Redraw {
        let in_slot =
            (0.0..self.bar_width as f64).contains(&x) && (0.0..BAR_HEIGHT as f64).contains(&y);
        if in_slot {
            self.open = !self.open;
            Redraw::Now
        } else {
            self.close()
        }
    }
}

impl Component for Media {
    fn bar_slot(&self) -> BarSlot {
        BarSlot::Center
    }

    fn bar_width(&self, state: &UIState, fonts: &FontBook) -> f32 {
        self.player(state).map_or(0.0, |player| {
            let text = bar_title(player, fonts);
            BOX_SIZE + ICON_GAP + fonts.measure(FONT, BAR_TEXT_SIZE, &text)
        })
    }

    /// Drawn here rather than in `draw` so the icon and the text can move into the island
    /// as it opens: the icon to the play button, the title to its first line.
    fn draw_overlay(
        &mut self,
        canvas: &Canvas,
        island: Option<IslandFrame>,
        state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> Redraw {
        self.bar_width = self.bar_width(state, fonts);
        let progress = island.map_or(0.0, |frame| frame.progress);
        let bar_text_at = Point::new(
            BOX_SIZE + ICON_GAP,
            bar_baseline(&fonts.sized(FONT, BAR_TEXT_SIZE)),
        );
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        // Fading over only half the way keeps what the two titles share solid throughout.
        let first_alpha = (progress * 2.0).min(1.0);
        let bar_alpha = ((1.0 - progress) * 2.0).min(1.0);
        // The lines are too close together to read until the island has made room for them.
        let rest_alpha = (progress * 2.0 - 1.0).max(0.0);

        let mut title_at = bar_text_at;
        let mut note_at = Rect::from_xywh(
            0.0,
            (BAR_HEIGHT as f32 - BOX_SIZE) / 2.0,
            BOX_SIZE,
            BOX_SIZE,
        );
        if let Some((frame, shown)) = island.zip(self.shown.as_ref()) {
            let origin = (frame.content.x, frame.content.y);
            for (i, (text, size, at)) in shown.lines(fonts).into_iter().enumerate() {
                let color = if i < shown.title.len() {
                    theme.on_surface
                } else {
                    theme.on_surface_variant
                };
                let at = travel(
                    bar_text_at,
                    at + Point::from(origin),
                    frame.content,
                    progress,
                );
                if i == 0 {
                    title_at = at;
                }
                let alpha = if i == 0 { first_alpha } else { rest_alpha };
                paint.set_color4f(Color4f::from(color.with_alpha(alpha)), None);
                fonts.draw(canvas, FONT, size, text, at, &paint);
            }
            let play = shown.button_box(1).with_offset(origin);
            note_at = lerp_box(note_at, play, progress);
        }

        if let Some(player) = self.player(state) {
            let color = if player.status == PlaybackStatus::Playing {
                theme.on_surface
            } else {
                theme.on_surface_variant
            };
            Icon::MusicNote.draw(canvas, note_at, color.with_alpha(1.0 - progress));
            paint.set_color4f(Color4f::from(color.with_alpha(bar_alpha)), None);
            let text = bar_title(player, fonts);
            fonts.draw(canvas, FONT, BAR_TEXT_SIZE, &text, title_at, &paint);
        }
        Redraw::None
    }

    fn on_cursor(&mut self, event: &PointerEvent) -> Redraw {
        match event.kind {
            PointerEventKind::Press { button, .. } if button == button::LEFT => {
                self.on_bar_press(event.position.0, event.position.1)
            }
            _ => Redraw::None,
        }
    }

    fn captures_outside_press(&self) -> bool {
        self.open
    }

    fn on_outside_press(&mut self) -> Redraw {
        self.close()
    }

    fn island(
        &mut self,
        state: &UIState,
        fonts: &FontBook,
        bar_width: f32,
    ) -> Option<IslandRequest> {
        // With the player gone there is nothing open to dismiss, so outside presses must
        // stop being captured too.
        let Some(player) = self.player(state) else {
            self.open = false;
            return None;
        };
        if !self.open {
            return None;
        }
        let shown = Shown::new(player, bar_width, fonts);
        let request = IslandRequest {
            width: shown.size.0,
            height: shown.size.1,
            placement: IslandPlacement::OverBar,
            priority: ISLAND_PRIORITY,
        };
        self.shown = Some(shown);
        Some(request)
    }

    fn draw_island(
        &mut self,
        canvas: &Canvas,
        _size: (f32, f32),
        _state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> Redraw {
        let Some(shown) = &self.shown else {
            return Redraw::None;
        };

        let mut paint = Paint::new(Color4f::from(theme.on_surface_variant), None);
        paint.set_anti_alias(true);

        // Mid-drag the slider leads, and the time follows it.
        self.slider.set_enabled(shown.progress.is_some());
        if let (Some((position, length)), false) = (shown.progress, self.slider.is_pressed()) {
            self.slider
                .set_value(position.as_secs_f32() / length.as_secs_f32());
        }
        let slider = shown.slider_rect();
        self.slider.set_layout_rect(slider);
        let animating = self.slider.draw(canvas, theme, fonts);

        if let Some((_, length)) = shown.progress {
            let position = length.mul_f32(self.slider.value());
            let baseline = slider.y + (slider.height + TIME_SIZE) / 2.0 - 1.0;
            let at = Point::new(PADDING, baseline);
            fonts.draw(canvas, FONT, TIME_SIZE, &format_time(position), at, &paint);
            let total = format_time(length);
            let x = shown.size.0 - PADDING - fonts.measure(FONT, TIME_SIZE, &total);
            let at = Point::new(x, baseline);
            fonts.draw(canvas, FONT, TIME_SIZE, &total, at, &paint);
        }

        for (i, action) in BUTTONS.iter().enumerate() {
            let icon = match action {
                PlayerAction::Previous => Icon::SkipPrevious,
                PlayerAction::Next => Icon::SkipNext,
                PlayerAction::PlayPause if shown.playing => Icon::Pause,
                _ => Icon::Play,
            };
            icon.draw(canvas, shown.button_box(i), theme.on_surface);
        }
        if animating {
            Redraw::Animating
        } else {
            Redraw::None
        }
    }

    fn on_island_cursor(&mut self, event: &PointerEvent) -> Redraw {
        match to_core_event(event) {
            Some(event) => self.on_island_pointer(&event),
            None => Redraw::None,
        }
    }

    fn on_mpris(&mut self, state: &PlayerState, _event: &Event) -> Redraw {
        if state.status == PlaybackStatus::Playing {
            self.last_playing = Some(state.bus_name.clone());
        }
        Redraw::Now
    }
}

#[cfg(test)]
mod tests {
    use mpris::LoopStatus;

    use crate::config::WatchableConfig;

    use super::*;

    fn player(bus_name: &str, status: PlaybackStatus) -> (String, PlayerState) {
        let state = PlayerState {
            bus_name: bus_name.to_string(),
            identity: bus_name.to_string(),
            active: true,
            status,
            title: None,
            artists: None,
            length: None,
            position: Duration::ZERO,
            position_at: Instant::now(),
            volume: None,
            shuffle: false,
            loop_status: LoopStatus::None,
        };
        (bus_name.to_string(), state)
    }

    fn shown<'a>(
        players: &'a HashMap<String, PlayerState>,
        last_playing: Option<&str>,
    ) -> Option<&'a str> {
        current_player(players, last_playing).map(|player| player.bus_name.as_str())
    }

    #[test]
    fn playing_beats_paused_and_stopped_never_shows() {
        let players = HashMap::from([
            player("a", PlaybackStatus::Paused),
            player("b", PlaybackStatus::Playing),
            player("c", PlaybackStatus::Stopped),
        ]);
        assert_eq!(shown(&players, Some("a")), Some("b"));

        let players = HashMap::from([player("c", PlaybackStatus::Stopped)]);
        assert_eq!(shown(&players, Some("c")), None);
    }

    #[test]
    fn pausing_stays_on_the_same_player() {
        let players = HashMap::from([
            player("a", PlaybackStatus::Paused),
            player("b", PlaybackStatus::Paused),
        ]);
        assert_eq!(shown(&players, Some("b")), Some("b"));
        assert_eq!(shown(&players, None), Some("a"), "stable with no history");
    }

    fn shown_with(progress: Option<(Duration, Duration)>) -> Shown {
        Shown {
            bus_name: "player".to_string(),
            title: Vec::new(),
            artist: String::new(),
            playing: true,
            progress,
            size: (360.0, 100.0),
        }
    }

    /// Presses and releases the slider, and returns what that sent.
    fn drag_slider(progress: Option<(Duration, Duration)>) -> Vec<PlayerCommand> {
        let theme = crate::config::theme::Theme::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("example/theme.toml"),
        );
        let mut fonts = FontBook::new();
        fonts.register(FONT, "Noto Sans CJK JP", skia_safe::FontStyle::normal());
        let mut surface = skia_safe::surfaces::raster_n32_premul((400, 200)).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut media = Media::new(tx);
        media.shown = Some(shown_with(progress));
        media.draw_island(
            surface.canvas(),
            (360.0, 100.0),
            &UIState::default(),
            &fonts,
            theme.theme(),
        );

        let rect = media.shown.as_ref().unwrap().slider_rect();
        let at = (
            (rect.x + rect.width / 2.0) as f64,
            (rect.y + rect.height / 2.0) as f64,
        );
        let left = button::LEFT;
        media.on_island_pointer(&CorePointerEvent::new(at, CoreKind::Press { button: left }));
        assert!(rx.try_recv().is_err(), "nothing until it is let go");
        media.on_island_pointer(&CorePointerEvent::new(
            at,
            CoreKind::Release { button: left },
        ));
        rx.try_iter().collect()
    }

    #[test]
    fn letting_go_of_the_slider_seeks() {
        let progress = (Duration::from_secs(10), Duration::from_secs(100));
        let commands = drag_slider(Some(progress));
        let [PlayerCommand {
            bus_name,
            action: PlayerAction::SeekTo(target),
        }] = commands.as_slice()
        else {
            panic!("expected one seek, got {commands:?}");
        };
        assert_eq!(bus_name, "player");
        assert!((45..55).contains(&target.as_secs()), "seeked to {target:?}");
    }

    #[test]
    fn a_player_without_a_length_cannot_seek() {
        assert_eq!(drag_slider(None), []);
    }

    #[test]
    fn text_waits_in_the_bar_then_rises_with_the_island() {
        let content = LayoutRect::new(-100.0, -100.0, 300.0, 160.0);
        let (from, to) = (Point::new(30.0, 36.0), Point::new(10.0, -70.0));

        assert_eq!(travel(from, to, content, 0.0), from);
        assert_eq!(travel(from, to, content, 1.0), to);
        assert_eq!(travel(from, to, content, 0.1).y, from.y, "still in the bar");

        // Its place in the island, not a straight line that would outrun the island's top.
        let at = travel(from, to, content, 0.5);
        assert_eq!(at, Point::new(20.0, 60.0 - 130.0 * 0.5));
        assert!(at.y > 60.0 - 160.0 * 0.5, "below the island's top");
    }

    #[test]
    fn times_read_as_minutes_and_seconds() {
        assert_eq!(format_time(Duration::from_secs(0)), "0:00");
        assert_eq!(format_time(Duration::from_secs(65)), "1:05");
        assert_eq!(format_time(Duration::from_secs(3725)), "62:05");
    }

    #[test]
    fn presses_map_to_buttons_only_inside_them() {
        let shown = shown_with(None);
        let (left, top) = shown.button_origin();
        let (left, y) = (left as f64, top as f64 + 1.0);
        let button = BUTTON_WIDTH as f64;

        assert_eq!(shown.button_at(left, y), Some(PlayerAction::Previous));
        assert_eq!(
            shown.button_at(left + button * 1.5, y),
            Some(PlayerAction::PlayPause)
        );
        assert_eq!(
            shown.button_at(left + button * 3.0 - 1.0, y),
            Some(PlayerAction::Next)
        );
        assert_eq!(shown.button_at(left + button * 3.0, y), None);
        assert_eq!(shown.button_at(left - 1.0, y), None);
        assert_eq!(shown.button_at(left, top as f64 - 1.0), None, "the text");
    }

    #[test]
    fn the_slot_toggles_and_presses_elsewhere_only_close() {
        let mut media = Media::new(std::sync::mpsc::channel().0);
        media.bar_width = 100.0;

        assert_eq!(media.on_bar_press(-500.0, 30.0), Redraw::None);
        assert!(!media.captures_outside_press());

        assert_eq!(media.on_bar_press(50.0, 30.0), Redraw::Now);
        assert!(media.captures_outside_press(), "the slot opens");

        assert_eq!(media.on_bar_press(50.0, 30.0), Redraw::Now);
        assert!(!media.open, "the slot closes");

        media.open = true;
        assert_eq!(media.on_bar_press(-500.0, 30.0), Redraw::Now);
        assert!(!media.open, "the rest of the bar closes");

        media.open = true;
        assert_eq!(media.on_outside_press(), Redraw::Now);
        assert!(!media.open, "outside the bar closes");
    }

    #[test]
    fn the_island_closes_itself_when_the_player_goes_away() {
        let mut fonts = FontBook::new();
        fonts.register(FONT, "Noto Sans CJK JP", skia_safe::FontStyle::normal());
        let mut media = Media::new(std::sync::mpsc::channel().0);
        media.open = true;

        let mut state = UIState::default();
        let (name, playing) = player("a", PlaybackStatus::Playing);
        state.players.insert(name, playing);
        assert!(media.island(&state, &fonts, 1000.0).is_some());
        assert!(media.captures_outside_press());

        state.players.clear();
        assert!(media.island(&state, &fonts, 1000.0).is_none());
        assert!(!media.captures_outside_press());
    }
}
