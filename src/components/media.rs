//! The media player in the middle of the bar: the cover art or an icon and the title, which
//! open an island with a card like Material 3 Expressive's media player on a phone.

use std::{
    collections::HashMap,
    sync::{mpsc::Sender, Arc},
    time::{Duration, Instant},
};

use m3_widget::{icon::BOX_SIZE, Icon, Slider, SliderWave, Widget};
use mpris::{Event, PlaybackStatus};
use skia_safe::{
    canvas::SrcRectConstraint,
    gradient::{shaders, Colors, Gradient, Interpolation},
    Canvas, Color4f, Contains, FilterMode, Image, MipmapMode, Paint, Point, RRect, Rect,
    SamplingOptions, TileMode,
};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};

use ui_core::{
    animation::{
        animation::{easing::ease_out_cubic, Animation},
        parser::Easing,
    },
    font::FontBook,
    geometry::LayoutRect,
    pointer::{button, PointerEvent as CorePointerEvent, PointerEventKind as CoreKind},
    scheme::{color::Color, ColorTheme},
    util::{elide, wrap_text},
};

use crate::{
    components::{toolbox::lerp_box, update_slider_easing},
    dbus::{
        mpris::{PlayerAction, PlayerCommand, PlayerState},
        player_art::Art,
    },
    ui::{
        bar_baseline, to_core_event, BarSlot, Component, IslandFrame, IslandPlacement,
        IslandRequest, Redraw, UIState, BAR_HEIGHT, ISLAND_SIDE_MARGIN,
    },
};

const FONT: &str = "noto_sans";
const BAR_TEXT_SIZE: f32 = 16.0;
const MAX_BAR_TEXT_WIDTH: f32 = 280.0;
const ICON_GAP: f32 = 8.0;
const THUMBNAIL_RADIUS: f32 = 6.0;

const ISLAND_WIDTH: f32 = 400.0;
/// The card sits inside the island, concentric with the island's corners.
const CARD_MARGIN: f32 = 8.0;
const CARD_RADIUS: f32 = 20.0;
const CARD_PADDING: f32 = 16.0;
/// Room for the art to show, however short the text is.
const HEADER_MIN_HEIGHT: f32 = 76.0;
const TITLE_SIZE: f32 = 16.0;
const ARTIST_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 1.4;
const MAX_TITLE_LINES: usize = 2;
const PLAY_SIZE: f32 = 56.0;
const PLAY_GAP: f32 = 12.0;
/// A circle at rest; playing, it's the squarer shape M3 Expressive morphs a button into.
const PLAY_RADIUS_PLAYING: f32 = 16.0;
const CONTROL_ROW_GAP: f32 = 8.0;
const CONTROL_SIZE: f32 = 40.0;
const CONTROL_INSET: f32 = 8.0;
const TIME_SIZE: f32 = 12.0;
const TIME_WIDTH: f32 = 36.0;
const SLIDER_HEIGHT: f32 = 24.0;
const WAVE_AMPLITUDE: f32 = 3.0;
/// How fast the wave travels, in radians a second: a wavelength every two seconds.
const WAVE_SPEED: f32 = std::f32::consts::PI;
const MORPH: Duration = Duration::from_millis(300);
const PLAY_PAUSE_EASING_ID: &str = "media_play_pause";
/// How long a seek gets to show up in what the player reports. A little over the time
/// between polling its position.
const SEEK_SETTLE: Duration = Duration::from_secs(2);
/// Below notifications, which take the island over and then hand it back.
const ISLAND_PRIORITY: u8 = 50;
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

/// Draws `image` over `rect`, cropped rather than stretched to fit.
fn draw_art(canvas: &Canvas, image: &Image, rect: Rect, radius: f32, alpha: f32) {
    let (width, height) = (image.width() as f32, image.height() as f32);
    let scale = (width / rect.width()).min(height / rect.height());
    let (crop_w, crop_h) = (rect.width() * scale, rect.height() * scale);
    let crop = Rect::from_xywh(
        (width - crop_w) / 2.0,
        (height - crop_h) / 2.0,
        crop_w,
        crop_h,
    );

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_alpha_f(alpha);
    let saved = canvas.save();
    canvas.clip_rrect(RRect::new_rect_xy(rect, radius, radius), None, true);
    canvas.draw_image_rect_with_sampling_options(
        image,
        Some((&crop, SrcRectConstraint::Fast)),
        rect,
        SamplingOptions::new(FilterMode::Linear, MipmapMode::None),
        &paint,
    );
    canvas.restore_to_count(saved);
}

/// The card's colors: from the art with light text on its scrim, whatever the theme is
/// like, and from the theme without art.
struct CardColors {
    content: Color,
    content_variant: Color,
    accent: Color,
    on_accent: Color,
    track: Color,
}

impl CardColors {
    fn new(art: Option<&Art>, theme: &ColorTheme) -> Self {
        match art {
            Some(art) => {
                let white = Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                };
                Self {
                    content: white,
                    content_variant: white.with_alpha(0.8),
                    accent: art.accent,
                    on_accent: art.on_accent,
                    track: white.with_alpha(0.3),
                }
            }
            None => Self {
                content: theme.on_surface,
                content_variant: theme.on_surface_variant,
                accent: theme.primary,
                on_accent: theme.on_primary,
                track: theme.secondary_container,
            },
        }
    }
}

/// The track laid out for the island. Kept after the island closes, so it is still there
/// to draw while it collapses.
struct Shown {
    bus_name: String,
    art: Option<Art>,
    title: Vec<String>,
    artist: String,
    playing: bool,
    /// `(position, length)`, if the player reports them.
    progress: Option<(Duration, Duration)>,
    size: (f32, f32),
}

impl Shown {
    fn new(player: &PlayerState, art: Option<&Art>, bar_width: f32, fonts: &FontBook) -> Self {
        let width = ISLAND_WIDTH.min(bar_width - ISLAND_SIDE_MARGIN * 2.0);
        let text_width = width - (CARD_MARGIN + CARD_PADDING) * 2.0 - PLAY_SIZE - PLAY_GAP;
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
        let text_height = TITLE_SIZE * LINE_HEIGHT * title.len() as f32 + artist_height;
        let height = (CARD_MARGIN + CARD_PADDING) * 2.0
            + text_height.max(HEADER_MIN_HEIGHT)
            + CONTROL_ROW_GAP
            + CONTROL_SIZE
            - CONTROL_INSET;
        Self {
            bus_name: player.bus_name.clone(),
            art: art.cloned(),
            title,
            artist,
            playing: player.status == PlaybackStatus::Playing,
            progress: player.progress(Instant::now()),
            size: (width, height),
        }
    }

    fn card(&self) -> Rect {
        Rect::from_wh(self.size.0, self.size.1).with_inset((CARD_MARGIN, CARD_MARGIN))
    }

    /// The title's and artist's lines: their text, size and baseline's start.
    fn lines(&self) -> Vec<(&str, f32, Point)> {
        let titles = self.title.iter().map(|line| (line.as_str(), TITLE_SIZE));
        let mut y = CARD_MARGIN + CARD_PADDING;
        titles
            .chain([(self.artist.as_str(), ARTIST_SIZE)])
            .filter(|(text, _)| !text.is_empty())
            .map(|(text, size)| {
                let at = Point::new(CARD_MARGIN + CARD_PADDING, y + size);
                y += size * LINE_HEIGHT;
                (text, size, at)
            })
            .collect()
    }

    fn play_rect(&self) -> Rect {
        let card = self.card();
        Rect::from_xywh(
            card.right - CARD_PADDING - PLAY_SIZE,
            card.top + CARD_PADDING,
            PLAY_SIZE,
            PLAY_SIZE,
        )
    }

    fn control_top(&self) -> f32 {
        self.card().bottom - CONTROL_INSET - CONTROL_SIZE
    }

    fn previous_rect(&self) -> Rect {
        let left = self.card().left + CONTROL_INSET;
        Rect::from_xywh(left, self.control_top(), CONTROL_SIZE, CONTROL_SIZE)
    }

    fn next_rect(&self) -> Rect {
        let left = self.card().right - CONTROL_INSET - CONTROL_SIZE;
        Rect::from_xywh(left, self.control_top(), CONTROL_SIZE, CONTROL_SIZE)
    }

    fn slider_rect(&self) -> LayoutRect {
        let left = self.previous_rect().right + TIME_WIDTH;
        LayoutRect::new(
            left,
            self.control_top() + (CONTROL_SIZE - SLIDER_HEIGHT) / 2.0,
            self.next_rect().left - TIME_WIDTH - left,
            SLIDER_HEIGHT,
        )
    }

    fn button_at(&self, x: f64, y: f64) -> Option<PlayerAction> {
        [
            (self.previous_rect(), PlayerAction::Previous),
            (self.play_rect(), PlayerAction::PlayPause),
            (self.next_rect(), PlayerAction::Next),
        ]
        .into_iter()
        .find(|(rect, _)| rect.contains(Point::new(x as f32, y as f32)))
        .map(|(_, action)| action)
    }

    /// The art under a scrim for the text, or a plain card. The text is on the left, so
    /// that's where the scrim is darkest.
    fn draw_card(&self, canvas: &Canvas, theme: &ColorTheme) {
        let card = self.card();
        let rrect = RRect::new_rect_xy(card, CARD_RADIUS, CARD_RADIUS);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let Some(art) = &self.art else {
            paint.set_color4f(Color4f::from(theme.surface_container_high), None);
            canvas.draw_rrect(rrect, &paint);
            return;
        };
        draw_art(canvas, &art.image, card, CARD_RADIUS, 1.0);
        let colors = [
            Color4f::new(0.0, 0.0, 0.0, 0.78),
            Color4f::new(0.0, 0.0, 0.0, 0.35),
        ];
        let scrim = Gradient::new(
            Colors::new_evenly_spaced(&colors, TileMode::Clamp, None),
            Interpolation::default(),
        );
        paint.set_shader(shaders::linear_gradient(
            ((card.left, 0.0), (card.right, 0.0)),
            &scrim,
            None,
        ));
        canvas.draw_rrect(rrect, &paint);
    }
}

pub struct Media {
    commands: Sender<PlayerCommand>,
    last_playing: Option<String>,
    open: bool,
    shown: Option<Shown>,
    slider: Slider,
    /// Where we last seeked to, and when.
    seeked: Option<(Duration, Instant)>,
    /// 1 while playing: drives the play button's shape and the wave's height.
    playing: Animation<f32>,
    /// What the wave's motion counts from.
    started: Instant,
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
            seeked: None,
            playing: Animation::new(0.0, 0.0, MORPH, Arc::new(ease_out_cubic)),
            started: Instant::now(),
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
                    self.seeked = Some((target, Instant::now()));
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

    fn on_easing_updated(&mut self, id: &str, easing: &Easing) -> Redraw {
        if id == PLAY_PAUSE_EASING_ID {
            self.playing.set_easing(easing.clone());
        }
        update_slider_easing(&mut self.slider, id, easing);
        Redraw::None
    }

    fn bar_width(&self, state: &UIState, fonts: &FontBook) -> f32 {
        self.player(state).map_or(0.0, |player| {
            let text = bar_title(player, fonts);
            BOX_SIZE + ICON_GAP + fonts.measure(FONT, BAR_TEXT_SIZE, &text)
        })
    }

    /// Drawn here rather than in `draw` so everything can move into the island as it opens:
    /// the title to its first line, and the art to the card or the icon to the play button.
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
        let in_bar = Rect::from_xywh(
            0.0,
            (BAR_HEIGHT as f32 - BOX_SIZE) / 2.0,
            BOX_SIZE,
            BOX_SIZE,
        );
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        // Fading over only half the way keeps what the two titles share solid throughout.
        let first_alpha = (progress * 2.0).min(1.0);
        let bar_alpha = ((1.0 - progress) * 2.0).min(1.0);
        // The lines are too close together to read until the island has made room for them.
        let rest_alpha = (progress * 2.0 - 1.0).max(0.0);

        let mut title_at = bar_text_at;
        let mut card_at = in_bar;
        let mut play_at = in_bar;
        if let Some((frame, shown)) = island.zip(self.shown.as_ref()) {
            let origin = (frame.content.x, frame.content.y);
            let colors = CardColors::new(shown.art.as_ref(), theme);
            for (i, (text, size, at)) in shown.lines().into_iter().enumerate() {
                let color = if i < shown.title.len() {
                    colors.content
                } else {
                    colors.content_variant
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
            card_at = shown.card().with_offset(origin);
            play_at = shown.play_rect().with_offset(origin);
        }

        if let Some(player) = self.player(state) {
            let color = if player.status == PlaybackStatus::Playing {
                theme.on_surface
            } else {
                theme.on_surface_variant
            };
            match state.player_art.get(&player.bus_name) {
                Some(art) => {
                    let lerp = |from: f32, to: f32| from + (to - from) * progress;
                    let at = Rect::new(
                        lerp(in_bar.left, card_at.left),
                        lerp(in_bar.top, card_at.top),
                        lerp(in_bar.right, card_at.right),
                        lerp(in_bar.bottom, card_at.bottom),
                    );
                    let radius = lerp(THUMBNAIL_RADIUS, CARD_RADIUS);
                    // Gone by halfway, before it can be seen not to line up with the growing card.
                    draw_art(
                        canvas,
                        &art.image,
                        at,
                        radius,
                        (1.0 - progress * 2.0).max(0.0),
                    );
                }
                None => {
                    let at = lerp_box(in_bar, play_at, progress);
                    Icon::MusicNote.draw(canvas, at, color.with_alpha(1.0 - progress));
                }
            }
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
        let art = state.player_art.get(&player.bus_name);
        let shown = Shown::new(player, art, bar_width, fonts);
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

        shown.draw_card(canvas, theme);
        let colors = CardColors::new(shown.art.as_ref(), theme);
        let mut slider_theme = theme.clone();
        slider_theme.primary = colors.accent;
        slider_theme.secondary_container = colors.track;
        slider_theme.on_surface = colors.content;

        let target = if shown.playing { 1.0 } else { 0.0 };
        if self.playing.to() != target {
            self.playing.set_target(target);
        }
        let playing = self.playing.value();

        // Mid-drag the slider leads, and the time follows it.
        self.slider.set_enabled(shown.progress.is_some());
        if let (Some((position, length)), false) = (shown.progress, self.slider.is_pressed()) {
            // The player takes a moment over a seek, and until then still reports where it
            // was: what we asked for holds until it has had time to get there.
            self.seeked = self.seeked.filter(|(_, at)| at.elapsed() < SEEK_SETTLE);
            let position = match self.seeked {
                Some((target, at)) if shown.playing => target + at.elapsed(),
                Some((target, _)) => target,
                None => position,
            };
            self.slider
                .set_value(position.as_secs_f32() / length.as_secs_f32());
        }
        let slider = shown.slider_rect();
        self.slider.set_layout_rect(slider);
        let waving = shown.progress.is_some();
        self.slider.set_wave(Some(SliderWave {
            amplitude: if waving {
                WAVE_AMPLITUDE * playing
            } else {
                0.0
            },
            phase: self.started.elapsed().as_secs_f32() * WAVE_SPEED,
        }));
        let animating = self.slider.draw(canvas, &slider_theme, fonts);

        if let Some((_, length)) = shown.progress {
            let mut paint = Paint::new(Color4f::from(colors.content_variant), None);
            paint.set_anti_alias(true);
            let baseline = slider.y + (slider.height + TIME_SIZE) / 2.0 - 1.0;
            let position = format_time(length.mul_f32(self.slider.value()));
            let at = Point::new(shown.previous_rect().right, baseline);
            fonts.draw(canvas, FONT, TIME_SIZE, &position, at, &paint);
            let total = format_time(length);
            let x = shown.next_rect().left - fonts.measure(FONT, TIME_SIZE, &total);
            let at = Point::new(x, baseline);
            fonts.draw(canvas, FONT, TIME_SIZE, &total, at, &paint);
        }

        let centered = |rect: Rect| {
            Rect::from_xywh(
                rect.center_x() - BOX_SIZE / 2.0,
                rect.center_y() - BOX_SIZE / 2.0,
                BOX_SIZE,
                BOX_SIZE,
            )
        };
        Icon::SkipPrevious.draw(canvas, centered(shown.previous_rect()), colors.content);
        Icon::SkipNext.draw(canvas, centered(shown.next_rect()), colors.content);

        let play = shown.play_rect();
        let radius = PLAY_SIZE / 2.0 + (PLAY_RADIUS_PLAYING - PLAY_SIZE / 2.0) * playing;
        let mut paint = Paint::new(Color4f::from(colors.accent), None);
        paint.set_anti_alias(true);
        canvas.draw_rrect(RRect::new_rect_xy(play, radius, radius), &paint);
        let icon = if shown.playing {
            Icon::Pause
        } else {
            Icon::Play
        };
        icon.draw(canvas, centered(play), colors.on_accent);

        // ponytail: repaints the whole surface every frame while a track plays with the
        // card open. Damage only the card if that shows up on a profile.
        if animating || self.playing.is_traveling() || (shown.playing && waving) {
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

    fn on_mpris(&mut self, state: &PlayerState, event: &Event) -> Redraw {
        if matches!(event, Event::TrackChanged(_)) {
            self.seeked = None;
        }
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
            art: None,
            title: Vec::new(),
            artist: String::new(),
            playing: true,
            progress,
            size: (400.0, 170.0),
        }
    }

    fn draw_island(media: &mut Media) {
        let theme = crate::config::theme::Theme::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("example/theme.toml"),
        );
        let mut fonts = FontBook::new();
        fonts.register(FONT, "Noto Sans CJK JP", skia_safe::FontStyle::normal());
        let mut surface = skia_safe::surfaces::raster_n32_premul((400, 170)).unwrap();
        media.draw_island(
            surface.canvas(),
            (400.0, 170.0),
            &UIState::default(),
            &fonts,
            theme.theme(),
        );
    }

    /// Presses and releases the middle of the slider.
    fn dragged(
        progress: Option<(Duration, Duration)>,
    ) -> (Media, std::sync::mpsc::Receiver<PlayerCommand>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut media = Media::new(tx);
        media.shown = Some(shown_with(progress));
        draw_island(&mut media);

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
        (media, rx)
    }

    /// Presses and releases the slider, and returns what that sent.
    fn drag_slider(progress: Option<(Duration, Duration)>) -> Vec<PlayerCommand> {
        dragged(progress).1.try_iter().collect()
    }

    #[test]
    fn a_seek_holds_until_the_player_has_had_time_to_get_there() {
        let progress = (Duration::from_secs(10), Duration::from_secs(100));
        let (mut media, _rx) = dragged(Some(progress));

        // The player still says 10s.
        draw_island(&mut media);
        let held = media.slider.value();
        assert!((0.45..0.55).contains(&held), "fell back to {held}");

        let (target, _) = media.seeked.unwrap();
        media.seeked = Some((target, Instant::now() - SEEK_SETTLE));
        draw_island(&mut media);
        assert!((media.slider.value() - 0.1).abs() < 0.001);
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
        let center = |rect: Rect| (rect.center_x() as f64, rect.center_y() as f64);
        let at = |rect: Rect| {
            let (x, y) = center(rect);
            shown.button_at(x, y)
        };

        assert_eq!(at(shown.previous_rect()), Some(PlayerAction::Previous));
        assert_eq!(at(shown.play_rect()), Some(PlayerAction::PlayPause));
        assert_eq!(at(shown.next_rect()), Some(PlayerAction::Next));

        let slider = shown.slider_rect();
        let on_slider = (slider.x + slider.width / 2.0, slider.y + 1.0);
        assert_eq!(
            shown.button_at(on_slider.0 as f64, on_slider.1 as f64),
            None
        );
        assert_eq!(shown.button_at(30.0, 30.0), None, "the text");
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
