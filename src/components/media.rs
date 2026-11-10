//! The media player in the middle of the bar: the cover art or an icon and the title, which
//! open an island with a card like Material 3 Expressive's media player on a phone.

use std::{
    cell::Cell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

use anti_webui::prelude::*;
use anti_webui::style::{BackgroundSize, Length, LineHeight, Style};
use anti_webui_md3e::{components::sized_icon, icons, motion::fast_spatial};
use kurbo::BezPath;
use mpris::PlaybackStatus;
use ui_core::{
    scheme::{color::Color as ThemeColor, ColorTheme},
    util::{elide, wrap_text},
};

use crate::{
    dbus::{
        mpris::{PlayerAction, PlayerCommand, PlayerState},
        player_art::Art,
    },
    state::State,
    ui::{color, palette, BarSlot, IslandPart, IslandPlacement, IslandRequest, Part, ISLAND_SIDE_MARGIN},
};

const FONT: &str = "Noto Sans CJK JP";
const BAR_TEXT_SIZE: f32 = 16.0;
const MAX_BAR_TEXT_WIDTH: f32 = 280.0;
const ICON_SIZE: f32 = 24.0;
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
/// The seek bar is `m3_widget`'s extra-small slider with a wave: a thin track, and a handle as
/// tall as the bar.
const TRACK: f32 = 4.0;
const HANDLE_WIDTH: f32 = 4.0;
const HANDLE_WIDTH_PRESSED: f32 = 2.0;
const HANDLE_GAP: f32 = 6.0;
const STOP_DIAMETER: f32 = 4.0;
/// The stop indicator sits a corner of the extra-small slider's 16px track in from its end.
const STOP_INSET: f32 = 8.0;
const WAVE_AMPLITUDE: f32 = 3.0;
const WAVE_LENGTH: f32 = 28.0;
/// How far from the handle the wave settles into a flat line.
const WAVE_TAPER: f32 = 14.0;
const WAVE_STEP: f32 = 2.0;
/// Of the content color, for a seek bar that can't seek.
const DISABLED_CONTENT_OPACITY: f32 = 0.38;
const DISABLED_TRACK_OPACITY: f32 = 0.12;
/// How fast the wave travels, in radians a second: a wavelength every two seconds.
const WAVE_SPEED: f32 = std::f32::consts::PI;
/// ponytail: animation.toml's `media_play_pause` easing isn't read here; it is a function and
/// AntiWebUI's transitions take a cubic-bezier. This is ease-out-cubic, its default.
const MORPH: Transition = Transition::ms(300).cubic_bezier(0.33, 1.0, 0.68, 1.0);
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

fn format_time(time: Duration) -> String {
    let seconds = time.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// What the bar and the card show of the current player.
#[derive(Clone, Debug, PartialEq)]
struct Track {
    bus_name: String,
    /// Falls back on the player's name for a track with no title.
    title: String,
    artists: String,
    playing: bool,
    /// Without one there is nothing to seek through.
    length: Option<Duration>,
    player: PlayerState,
}

impl Track {
    fn new(player: &PlayerState) -> Self {
        Self {
            bus_name: player.bus_name.clone(),
            title: player
                .title
                .clone()
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| player.identity.clone()),
            artists: player.artists.as_deref().unwrap_or_default().join(", "),
            playing: player.status == PlaybackStatus::Playing,
            length: player.progress(Instant::now()).map(|(_, length)| length),
            player: player.clone(),
        }
    }

    /// Where playback is now, as the player last reported it.
    fn position(&self, now: Instant) -> Duration {
        self.player.progress(now).map_or(Duration::ZERO, |(position, _)| position)
    }
}

/// The card as laid out for a track: kept after the island closes, so it is still there
/// while it collapses.
#[derive(Clone, Debug, PartialEq)]
struct Card {
    title: Vec<String>,
    artist: String,
    size: (f32, f32),
}

fn text_style(size: f32) -> Style {
    Style::default().font_family(FONT).text_size(size)
}

fn measure(size: f32) -> impl Fn(&str) -> f32 {
    let style = text_style(size);
    move |s| svg::measure_text(s, &style).0
}

impl Card {
    fn new(track: &Track, bar_width: f32) -> Self {
        let width = ISLAND_WIDTH.min(bar_width - ISLAND_SIDE_MARGIN * 2.0);
        let text_width = width - (CARD_MARGIN + CARD_PADDING) * 2.0 - PLAY_SIZE - PLAY_GAP;
        let artist = elide(&track.artists, text_width, measure(ARTIST_SIZE));
        let title = wrap_text(&track.title, text_width, MAX_TITLE_LINES, measure(TITLE_SIZE));
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
            title,
            artist,
            size: (width, height),
        }
    }
}

/// The card's colors: from the art with light text on its scrim, whatever the theme is
/// like, and from the theme without art.
#[derive(Clone, Copy)]
struct CardColors {
    content: ThemeColor,
    content_variant: ThemeColor,
    accent: ThemeColor,
    on_accent: ThemeColor,
    track: ThemeColor,
}

impl CardColors {
    fn new(art: Option<&Art>, theme: &ColorTheme) -> Self {
        match art {
            Some(art) => {
                let white = ThemeColor {
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

/// A Material Symbol, filled like `m3_widget`'s icons.
fn filled_icon(symbol: &'static anti_webui_md3e::icons::Symbol) -> Div {
    sized_icon(symbol, ICON_SIZE).font_axis(b"FILL", 1.0)
}

/// Calls `f` every frame for as long as `active` reads true.
fn every_frame(active: Memo<bool>, f: impl Fn() + 'static) {
    fn step(active: Memo<bool>, f: Rc<dyn Fn()>, run: Rc<Cell<u64>>, me: u64) {
        request_frame(move |_| {
            if run.get() == me && untrack(|| active.get()) {
                f();
                step(active, f, run, me);
            }
        });
    }
    let f: Rc<dyn Fn()> = Rc::new(f);
    let run = Rc::new(Cell::new(0u64));
    effect(move || {
        if active.get() {
            // One that went off and on again before its next frame stops for this one.
            run.set(run.get() + 1);
            step(active, f.clone(), run.clone(), run.get());
        }
    });
}

/// The wavy progress line of M3 Expressive's media player: the played part waves while
/// `amplitude` is above 0, then a gap, the handle, a gap and the rest of the track.
/// The seek bar's shapes, as `m3_widget::Slider` drew it with a wave.
struct Progress {
    /// The played part as a line to stroke: it waves, and settles flat towards the handle so
    /// it never leaves the track pointing away from it.
    played: BezPath,
    /// The rest of the track, a thin bar.
    rest: BezPath,
    handle: BezPath,
    /// The stop indicator at the far end, and whether the played part has reached it.
    stop: Option<(BezPath, bool)>,
}

/// `handle_width` narrows while the handle is held.
fn progress(width: f32, fraction: f32, amplitude: f32, phase: f32, handle_width: f32) -> Progress {
    let f = f64::from;
    let mid = SLIDER_HEIGHT / 2.0;
    let handle = HANDLE_WIDTH / 2.0 + (width - HANDLE_WIDTH) * fraction;
    let played_end = (handle - handle_width / 2.0 - HANDLE_GAP).max(0.0);
    let rest_start = (handle + handle_width / 2.0 + HANDLE_GAP).min(width);

    let mut played = BezPath::new();
    if played_end > 0.0 {
        let y = |x: f32| {
            let settle = ((played_end - x) / WAVE_TAPER).min(1.0);
            let angle = x / WAVE_LENGTH * std::f32::consts::TAU - phase;
            f(mid + angle.sin() * amplitude * settle)
        };
        played.move_to((0.0, y(0.0)));
        let mut x = WAVE_STEP;
        while x < played_end {
            played.line_to((f(x), y(x)));
            x += WAVE_STEP;
        }
        played.line_to((f(played_end), y(played_end)));
    }
    let bar = |x0: f32, y0: f32, x1: f32, y1: f32, radius: f32| {
        kurbo::Shape::to_path(&kurbo::RoundedRect::new(f(x0), f(y0), f(x1), f(y1), f(radius)), 0.05)
    };
    let rest = if rest_start < width {
        bar(rest_start, mid - TRACK / 2.0, width, mid + TRACK / 2.0, TRACK / 2.0)
    } else {
        BezPath::new()
    };
    let handle_path = bar(
        handle - handle_width / 2.0,
        0.0,
        handle + handle_width / 2.0,
        SLIDER_HEIGHT,
        handle_width / 2.0,
    );
    // Dropped where the handle's gap would swallow it.
    let stop_x = width - STOP_INSET;
    let stop = ((stop_x - handle).abs() > handle_width / 2.0 + HANDLE_GAP).then(|| {
        let dot = kurbo::Circle::new((f(stop_x), f(mid)), f(STOP_DIAMETER / 2.0));
        (kurbo::Shape::to_path(&dot, 0.05), stop_x < handle)
    });
    Progress {
        played,
        rest,
        handle: handle_path,
        stop,
    }
}

pub fn media(state: &State, bar_width: f32) -> Part {
    let players = state.players;
    let player_art = state.player_art;
    let last_playing = state.last_playing;
    let commands = state.commands.clone();

    let track = memo(move || {
        let last = last_playing.get();
        players.with(|p| current_player(p, last.as_deref()).map(Track::new))
    });
    let art = memo(move || {
        let bus = track.with(|t| t.as_ref().map(|t| t.bus_name.clone()))?;
        player_art.with(|a| a.get(&bus).cloned())
    });
    let open = signal(false);
    // With the player gone there is nothing to show.
    effect(move || {
        if track.with(Option::is_none) {
            untrack(|| open.set(false));
        }
    });

    // The card for the island, kept while it collapses after the player has gone.
    let card = signal(None::<Card>);
    effect(move || {
        if let Some(t) = track.get() {
            let laid_out = Card::new(&t, bar_width);
            if untrack(|| card.get()).as_ref() != Some(&laid_out) {
                untrack(|| card.set(Some(laid_out)));
            }
        }
    });
    let request = memo(move || {
        if !open.get() || track.with(Option::is_none) {
            return None;
        }
        let (width, height) = card.with(|c| c.as_ref().map(|c| c.size))?;
        Some(IslandRequest {
            width,
            height,
            placement: IslandPlacement::OverBar,
            priority: ISLAND_PRIORITY,
        })
    });

    let send = {
        let commands = commands.clone();
        move |action: PlayerAction| {
            if let Some(t) = untrack(|| track.get()) {
                let _ = commands.player.send(PlayerCommand {
                    bus_name: t.bus_name,
                    action,
                });
            }
        }
    };

    let bar = bar_view(state, track, art, open);
    let content = island_view(state, track, art, card, open, send);
    Part {
        bar: Some((BarSlot::Center, bar.into_view())),
        island: Some(IslandPart {
            request,
            content: content.into_view(),
        }),
    }
}

/// The art or an icon, and the title, in the bar. A click toggles the island.
fn bar_view(state: &State, track: Memo<Option<Track>>, art: Memo<Option<Art>>, open: Signal<bool>) -> Div {
    let theme_state = state.clone();
    let color = move || {
        let playing = track.with(|t| t.as_ref().is_some_and(|t| t.playing));
        palette(&theme_state, |t| if playing { t.on_surface } else { t.on_surface_variant })
    };
    let icon_color = color.clone();
    let text_color = color;
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(ICON_GAP)
        .transition_of([Prop::Opacity], Transition::ms(150))
        // Under the card while it is open, which has the same in it.
        .dyn_style(move |s| {
            let s = if open.get() { s.opacity(0.0) } else { s.opacity(1.0) };
            if track.with(Option::is_none) { s.hidden() } else { s }
        })
        .on_click(move |_| open.update(|o| *o = !*o))
        .child(dyn_view(move || match art.get() {
            Some(art) => div()
                .w(ICON_SIZE)
                .h(ICON_SIZE)
                .rounded(THUMBNAIL_RADIUS)
                .bg_image(art.pixels.clone())
                .bg_size(BackgroundSize::Cover)
                .bg_position(Length::Percent(50.0), Length::Percent(50.0))
                .into_view(),
            None => {
                let icon_color = icon_color.clone();
                filled_icon(icons::MUSIC_NOTE)
                    .dyn_style(move |s| s.text_color(icon_color()))
                    .into_view()
            }
        }))
        .child(
            text(move || {
                let title = track.with(|t| t.as_ref().map(|t| t.title.clone()));
                elide(&title.unwrap_or_default(), MAX_BAR_TEXT_WIDTH, measure(BAR_TEXT_SIZE))
            })
            .text_size(BAR_TEXT_SIZE)
            .whitespace_nowrap()
            .dyn_style(move |s| s.text_color(text_color())),
        )
}

fn island_view(
    state: &State,
    track: Memo<Option<Track>>,
    art: Memo<Option<Art>>,
    card: Signal<Option<Card>>,
    open: Signal<bool>,
    send: impl Fn(PlayerAction) + Clone + 'static,
) -> Div {
    let theme = state.theme;
    let colors = move || theme.with(|t| CardColors::new(art.get().as_ref(), t.theme()));
    let playing = memo(move || track.with(|t| t.as_ref().is_some_and(|t| t.playing)));
    let play_morph = follow(move || if playing.get() { 1.0f32 } else { 0.0 }, MORPH);
    let card_size = move || card.with(|c| c.as_ref().map_or((0.0, 0.0), |c| c.size));
    let (inner_w, inner_h) = (
        move || card_size().0 - CARD_MARGIN * 2.0,
        move || card_size().1 - CARD_MARGIN * 2.0,
    );

    // The slider leads while it is held, and the player's position otherwise.
    let value = signal(0.0f32);
    let held = signal(false);
    let seeked = Rc::new(Cell::new(None::<(Duration, Instant)>));
    let sync = {
        let seeked = seeked.clone();
        move || {
            if untrack(|| held.get()) {
                return;
            }
            let Some(t) = untrack(|| track.get()) else {
                return;
            };
            let Some(length) = t.length else {
                return;
            };
            // The player takes a moment over a seek, and until then still reports where it
            // was: what we asked for holds until it has had time to get there.
            seeked.set(seeked.get().filter(|(_, at)| at.elapsed() < SEEK_SETTLE));
            let position = match seeked.get() {
                Some((target, at)) if t.playing => target + at.elapsed(),
                Some((target, _)) => target,
                None => t.position(Instant::now()),
            };
            value.set(position.as_secs_f32() / length.as_secs_f32());
        }
    };
    {
        let sync = sync.clone();
        let seeked = seeked.clone();
        let last_title = Cell::new(None::<(String, String)>);
        effect(move || {
            let key = track.with(|t| t.as_ref().map(|t| (t.bus_name.clone(), t.title.clone())));
            if last_title.replace(key.clone()) != key {
                seeked.set(None);
            }
            sync();
        });
    }
    // The slider only seeks once it is let go, since a player may take a moment over every seek.
    {
        let send = send.clone();
        let seeked = seeked.clone();
        let was = Cell::new(false);
        effect(move || {
            let now = held.get();
            if was.replace(now) && !now {
                if let Some(length) = untrack(|| track.get()).and_then(|t| t.length) {
                    let target = length.mul_f32(untrack(|| value.get()));
                    seeked.set(Some((target, Instant::now())));
                    send(PlayerAction::SeekTo(target));
                }
            }
        });
    }
    // While the card is open and the track plays, the wave flows and the position advances.
    let phase = signal(0.0f32);
    let started = Instant::now();
    every_frame(memo(move || open.get() && playing.get()), {
        let sync = sync.clone();
        move || {
            phase.set(started.elapsed().as_secs_f32() * WAVE_SPEED);
            sync();
        }
    });

    let can_seek = memo(move || track.with(|t| t.as_ref().is_some_and(|t| t.length.is_some())));
    let slider = slider_with(value, 0.0, 1.0, SliderOptions::new(), move |s: SliderState| {
        effect(move || {
            let now = s.held().is_some();
            untrack(|| held.set(now));
        });
        let width = signal(0.0f32);
        let handle_width = follow(
            move || if s.held().is_some() { HANDLE_WIDTH_PRESSED } else { HANDLE_WIDTH },
            fast_spatial(),
        );
        let shapes = Rc::new(move || {
            let amplitude = WAVE_AMPLITUDE * play_morph.get();
            progress(width.get(), s.fraction(), amplitude, phase.get(), handle_width.get())
        });
        // The played part, the handle and a stop on the rest of the track are the accent; the
        // rest, and a stop the played part has reached, are the track's color.
        let active = move || {
            let c = colors();
            if can_seek.get() {
                c.accent
            } else {
                c.content.with_alpha(DISABLED_CONTENT_OPACITY)
            }
        };
        let inactive = move || {
            let c = colors();
            if can_seek.get() {
                c.track
            } else {
                c.content.with_alpha(DISABLED_TRACK_OPACITY)
            }
        };
        let played = {
            let shapes = shapes.clone();
            svg::path(move || shapes().played)
                .fill_none()
                .stroke_width(TRACK)
                .line_cap(anti_webui::style::LineCap::Round)
                .line_join(anti_webui::style::LineJoin::Round)
                .dyn_style(move |st| st.stroke(color(active())))
        };
        let rest = {
            let shapes = shapes.clone();
            svg::path(move || shapes().rest).dyn_style(move |st| st.fill(color(inactive())))
        };
        let stop = {
            let shapes = shapes.clone();
            let on_played = {
                let shapes = shapes.clone();
                move || shapes().stop.is_some_and(|(_, on_played)| on_played)
            };
            svg::path(move || shapes().stop.map_or_else(BezPath::new, |(dot, _)| dot)).dyn_style(
                move |st| st.fill(color(if on_played() { inactive() } else { active() })),
            )
        };
        let handle = svg::path(move || shapes().handle).dyn_style(move |st| st.fill(color(active())));
        div()
            .relative()
            .flex_1()
            .h(SLIDER_HEIGHT)
            .on_resized(move |w, _| width.set(w))
            .child(
                // Out of the flow: an svg with no size of its own would ask for 300px.
                svg()
                    .absolute()
                    .left(0.0)
                    .top(0.0)
                    .size_full()
                    .child(played)
                    .child(rest)
                    .child(stop)
                    .child(handle),
            )
    });

    let time = |f: Box<dyn Fn() -> String>| {
        text(f)
            .text_size(TIME_SIZE)
            .whitespace_nowrap()
            .dyn_style(move |s| s.text_color(color(colors().content_variant)))
    };
    let position_text = time(Box::new(move || {
        track.with(|t| {
            t.as_ref()
                .and_then(|t| t.length)
                .map_or(String::new(), |l| format_time(l.mul_f32(value.get())))
        })
    }));
    let length_text = time(Box::new(move || {
        track.with(|t| t.as_ref().and_then(|t| t.length).map_or(String::new(), format_time))
    }));
    // A fixed width keeps the seek bar from moving as the digits change. The position starts
    // where the previous button's box ends, and the length ends where the next one's begins.
    let time_box = |time| div().w(TIME_WIDTH).flex_none().flex().child(time);

    let control = |symbol, action: PlayerAction| {
        let send = send.clone();
        div()
            .w(CONTROL_SIZE)
            .h(CONTROL_SIZE)
            .flex()
            .items_center()
            .justify_center()
            .rounded(CONTROL_SIZE / 2.0)
            .on_click(move |_| send(action.clone()))
            .child(
                filled_icon(symbol)
                    .dyn_style(move |s| s.text_color(color(colors().content))),
            )
    };

    let play = {
        let send = send.clone();
        div()
            .w(PLAY_SIZE)
            .h(PLAY_SIZE)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .on_click(move |_| send(PlayerAction::PlayPause))
            .dyn_style(move |s| {
                let radius = PLAY_SIZE / 2.0 + (PLAY_RADIUS_PLAYING - PLAY_SIZE / 2.0) * play_morph.get();
                s.rounded(radius).bg(color(colors().accent))
            })
            .child(dyn_view(move || {
                let symbol = if playing.get() { icons::PAUSE } else { icons::PLAY_ARROW };
                filled_icon(symbol)
                    .dyn_style(move |s| s.text_color(color(colors().on_accent)))
                    .into_view()
            }))
    };

    let lines = div().flex().flex_col().flex_1().child(dyn_view(move || {
        let Some(c) = card.get() else {
            return div().into_view();
        };
        let mut column = div().flex().flex_col();
        for line in c.title {
            column = column.child(
                text(line)
                    .text_size(TITLE_SIZE)
                    .line_height(LineHeight::Relative(LINE_HEIGHT))
                    .whitespace_nowrap()
                    .dyn_style(move |s| s.text_color(color(colors().content))),
            );
        }
        if !c.artist.is_empty() {
            column = column.child(
                text(c.artist)
                    .text_size(ARTIST_SIZE)
                    .line_height(LineHeight::Relative(LINE_HEIGHT))
                    .whitespace_nowrap()
                    .dyn_style(move |s| s.text_color(color(colors().content_variant))),
            );
        }
        column.into_view()
    }));

    // The art under a scrim for the text, or a plain card. The text is on the left, so
    // that's where the scrim is darkest.
    let scrim = svg()
        .absolute()
        .left(0.0)
        .top(0.0)
        .size_full()
        .dyn_style(move |s| if art.with(Option::is_some) { s } else { s.hidden() })
        .child(
            svg::path(move || {
                kurbo::Shape::to_path(
                    &kurbo::RoundedRect::new(0.0, 0.0, f64::from(inner_w()), f64::from(inner_h()), f64::from(CARD_RADIUS)),
                    0.05,
                )
            })
            .fill(
                Gradient::linear(0.0, 0.0, 1.0, 0.0)
                    .stop(0.0, Color::new(0.0, 0.0, 0.0, 0.78))
                    .stop(1.0, Color::new(0.0, 0.0, 0.0, 0.35)),
            ),
        );

    let state = state.clone();
    div().size_full().p(CARD_MARGIN).child(
        div()
            .relative()
            .size_full()
            .rounded(CARD_RADIUS)
            .overflow_hidden()
            .dyn_style(move |s| match art.get() {
                Some(art) => s.bg_image(art.pixels.clone()).bg_size(BackgroundSize::Cover)
                .bg_position(Length::Percent(50.0), Length::Percent(50.0)),
                None => s.bg(palette(&state, |t| t.surface_container_high)),
            })
            .child(scrim)
            .child(
                div()
                    .absolute()
                    .left(CARD_PADDING)
                    .top(CARD_PADDING)
                    .right(CARD_PADDING)
                    .flex()
                    .flex_row()
                    .gap(PLAY_GAP)
                    .child(lines)
                    .child(play),
            )
            .child(
                div()
                    .absolute()
                    .left(CONTROL_INSET)
                    .right(CONTROL_INSET)
                    .bottom(CONTROL_INSET)
                    .h(CONTROL_SIZE)
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(control(icons::SKIP_PREVIOUS, PlayerAction::Previous))
                    .child(time_box(position_text).justify_start())
                    .child(slider.flex_1().min_w(0.0))
                    .child(time_box(length_text).justify_end())
                    .child(control(icons::SKIP_NEXT, PlayerAction::Next)),
            ),
    )
}

#[cfg(test)]
mod tests {
    use mpris::LoopStatus;

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

    #[test]
    fn times_read_as_minutes_and_seconds() {
        assert_eq!(format_time(Duration::from_secs(0)), "0:00");
        assert_eq!(format_time(Duration::from_secs(65)), "1:05");
        assert_eq!(format_time(Duration::from_secs(3725)), "62:05");
    }

    #[test]
    fn the_progress_line_leaves_a_gap_around_the_handle() {
        let p = progress(200.0, 0.5, 0.0, 0.0, HANDLE_WIDTH);
        let x = |p: &BezPath| kurbo::Shape::bounding_box(p);
        let handle = (x(&p.handle).x0 + x(&p.handle).x1) as f32 / 2.0;
        assert!((handle - 100.0).abs() < 0.01, "{handle}");
        assert!(x(&p.played).x1 as f32 <= handle - HANDLE_WIDTH / 2.0 - HANDLE_GAP + 0.01);
        assert!(x(&p.rest).x0 as f32 >= handle + HANDLE_WIDTH / 2.0 + HANDLE_GAP - 0.01);
        assert!(p.stop.is_some_and(|(_, played)| !played), "the stop is ahead");

        let p = progress(200.0, 0.0, 3.0, 0.0, HANDLE_WIDTH);
        assert!(p.played.elements().is_empty(), "nothing played at the start");
    }

    #[test]
    fn the_wave_settles_flat_at_the_handle_and_the_stop_goes_under_it() {
        let p = progress(200.0, 0.5, 3.0, 0.0, HANDLE_WIDTH);
        let end = p.played.elements().last().and_then(|e| e.end_point()).unwrap();
        assert_eq!(end.y as f32, SLIDER_HEIGHT / 2.0, "flat where it meets the gap");

        // At the end the handle's gap swallows the stop.
        assert!(progress(200.0, 1.0, 3.0, 0.0, HANDLE_WIDTH).stop.is_none());
    }
}
