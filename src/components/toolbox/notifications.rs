//! The toolbox's other panel, opened by a right click on its row: the notifications
//! nobody has dismissed yet, including the ones that expired unseen. Laid out like
//! Android's shade: one rounded section of rows, each app's notifications stacked
//! into one row until it is clicked open. Rows go by dragging them sideways, by a
//! sideways touchpad swipe, or by the ✕ that shows on hover. The list scrolls in a
//! panel three rows tall.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use m3_widget::Icon;
use skia_safe::{
    Canvas, Color4f, FilterMode, Image, MipmapMode, Paint, PaintCap, PaintJoin, PaintStyle,
    PathBuilder, Point, RRect, Rect, SamplingOptions, Vector,
};

use ui_core::{
    animation::animation::{easing::ease_out_cubic, Animation},
    font::FontBook,
    geometry::LayoutRect,
    scheme::ColorTheme,
    util::elide,
};

use super::{PANEL_PADDING, PANEL_WIDTH};
use crate::dbus::notification::{ActiveNotification, NotificationStore, Urgency};

const FONT: &str = "noto_sans";
const TOP_PADDING: f32 = 12.0;
const HEADER_HEIGHT: f32 = 32.0;
const HEADER_GAP: f32 = 12.0;
const TITLE_SIZE: f32 = 16.0;
const SMALL_SIZE: f32 = 12.0;
const TEXT_SIZE: f32 = 14.0;
const CLEAR_SIZE: f32 = 13.0;
const LINE_HEIGHT: f32 = 1.4;
/// Between rows of the one section.
const ROW_GAP: f32 = 2.0;
const OUTER_RADIUS: f32 = 20.0;
const INNER_RADIUS: f32 = 6.0;
const ROW_PADDING: f32 = 14.0;
/// A notification, or an app's notifications stacked into one.
const CARD_HEIGHT: f32 = 84.0;
/// An open app's heading.
const HEAD_HEIGHT: f32 = 56.0;
/// One notification under an open app's heading.
const CHILD_HEIGHT: f32 = 64.0;
const CHILD_PADDING: f32 = 12.0;
const ICON_SIZE: f32 = 40.0;
const TEXT_X: f32 = ROW_PADDING + ICON_SIZE + ROW_PADDING;
const BUTTON_SIZE: f32 = 28.0;
const BUTTON_ICON: f32 = 14.0;
const PILL_HEIGHT: f32 = 24.0;
const CHEVRON: f32 = 16.0;
const LIST_TOP: f32 = TOP_PADDING + HEADER_HEIGHT + HEADER_GAP;
/// Below the last row, inside the scrolling list.
const LIST_BOTTOM: f32 = 12.0;
const ROW_WIDTH: f32 = PANEL_WIDTH - PANEL_PADDING * 2.0;
/// Always three cards tall, whatever there is.
pub const PANEL_HEIGHT: f32 = LIST_TOP + CARD_HEIGHT * 3.0 + ROW_GAP * 2.0 + LIST_BOTTOM;
/// Pixels per unit of the compositor's axis value.
/// ponytail: one speed for wheels and touchpads; split on `source` if either feels off.
const SCROLL_SPEED: f32 = 1.0;
/// How far a press moves before it counts as a drag rather than a click.
const DRAG_SLOP: f32 = 6.0;
/// How much of its width a row must be dragged to go once let go.
const DISMISS_RATIO: f32 = 0.4;
const SLIDE: Duration = Duration::from_millis(200);
/// How long rows take to close a gap, or to make room for an app opening.
const MOVE: Duration = Duration::from_millis(250);
/// "すべて消去" slides the rows in view away one after another, from the top, this far
/// apart.
const CLEAR_STAGGER: Duration = Duration::from_millis(50);

pub enum Action {
    /// Every id in a row: one, or a stacked app's.
    Dismiss(Vec<u32>),
    /// A group opened or closed.
    Repaint,
}

/// "5分前" and the like.
fn ago(elapsed: Duration) -> String {
    match elapsed.as_secs() / 60 {
        0 => "たった今".to_string(),
        minutes @ 1..60 => format!("{minutes}分前"),
        minutes @ 60..1440 => format!("{}時間前", minutes / 60),
        minutes => format!("{}日前", minutes / 1440),
    }
}

/// `unseen` by app, each app where its newest notification is, newest first.
fn group_by_app<'a>(unseen: &[&'a ActiveNotification]) -> Vec<Vec<&'a ActiveNotification>> {
    let mut groups: Vec<Vec<&ActiveNotification>> = Vec::new();
    for &notification in unseen {
        let app = &notification.data.app_name;
        match groups.iter_mut().find(|g| &g[0].data.app_name == app) {
            Some(group) => group.push(notification),
            None => groups.push(vec![notification]),
        }
    }
    groups
}

fn paint(color: impl Into<Color4f>) -> Paint {
    let mut paint = Paint::new(color.into(), None);
    paint.set_anti_alias(true);
    paint
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    /// A notification, or with `group_len > 1` an app's, stacked.
    Card,
    /// An open app's heading, with its notifications as `Child`ren below.
    Head,
    Child,
}

/// Which row, across layouts: a group's heading and its first child share an id.
type RowKey = (u32, Kind);

struct Row {
    kind: Kind,
    /// What dismissing it dismisses: one, or a stacked app's.
    ids: Vec<u32>,
    app: String,
    group_len: usize,
    critical: bool,
    icon: Option<Image>,
    header: String,
    title: String,
    text: String,
    /// In list coordinates: the list's top is 0, before scrolling.
    rect: LayoutRect,
    close: LayoutRect,
    /// The count and chevron, for a row that opens or closes a group.
    pill: Option<(LayoutRect, String)>,
}

impl Row {
    fn new(
        kind: Kind,
        notification: &ActiveNotification,
        group: &[&ActiveNotification],
        y: f32,
        now: Instant,
        fonts: &FontBook,
    ) -> Self {
        let data = &notification.data;
        let height = match kind {
            Kind::Card => CARD_HEIGHT,
            Kind::Head => HEAD_HEIGHT,
            Kind::Child => CHILD_HEIGHT,
        };
        let rect = LayoutRect::new(PANEL_PADDING, y, ROW_WIDTH, height);
        let right = rect.x + rect.width - ROW_PADDING;
        let top = rect.y + ROW_PADDING;

        let pill = (kind == Kind::Head || (kind == Kind::Card && group.len() > 1)).then(|| {
            let count = group.len().to_string();
            let width = 10.0 + fonts.measure(FONT, SMALL_SIZE, &count) + 2.0 + CHEVRON + 6.0;
            let y = top + (BUTTON_SIZE - PILL_HEIGHT) / 2.0;
            (LayoutRect::new(right - width, y, width, PILL_HEIGHT), count)
        });
        let close_right = pill.as_ref().map_or(right, |(rect, _)| rect.x - 4.0);
        let close = LayoutRect::new(close_right - BUTTON_SIZE, top, BUTTON_SIZE, BUTTON_SIZE);

        let ago = ago(now.saturating_duration_since(notification.posted));
        let (header, title) = match kind {
            Kind::Card => (format!("{} · {ago}", data.app_name), data.summary.clone()),
            Kind::Head => (
                format!("{} · {}件", data.app_name, group.len()),
                String::new(),
            ),
            Kind::Child => (String::new(), format!("{} · {ago}", data.summary)),
        };
        let text = data.body.lines().next().unwrap_or_default().to_string();
        // The close button only shows on hover, but the text stops short of it anyway,
        // so that hovering doesn't reflow it.
        let width = close.x - 8.0 - (rect.x + TEXT_X);
        let fit = |text: &str, size: f32| elide(text, width, |s| fonts.measure(FONT, size, s));
        Self {
            kind,
            ids: if kind == Kind::Child {
                vec![notification.id]
            } else {
                group.iter().map(|n| n.id).collect()
            },
            app: data.app_name.clone(),
            group_len: group.len(),
            critical: data.urgency == Urgency::Critical,
            icon: data.icon.clone(),
            header: fit(&header, SMALL_SIZE),
            title: fit(&title, TEXT_SIZE),
            text: fit(&text, TEXT_SIZE),
            rect,
            close,
            pill,
        }
    }

    fn key(&self) -> RowKey {
        (self.ids[0], self.kind)
    }

    /// Whether clicking it opens or closes its app.
    fn toggles(&self) -> bool {
        self.group_len > 1 && self.kind != Kind::Child
    }

    fn draw(
        &self,
        canvas: &Canvas,
        fonts: &FontBook,
        theme: &ColorTheme,
        (first, last): (bool, bool),
        hovered: bool,
        offset: f32,
    ) {
        let saved = canvas.save();
        if offset != 0.0 {
            let fade = (offset.abs() / self.rect.width).min(1.0);
            canvas.save_layer_alpha_f(None, 1.0 - fade * 0.8);
            canvas.translate((offset, 0.0));
        }

        let r = self.rect;
        let top = if first { OUTER_RADIUS } else { INNER_RADIUS };
        let bottom = if last { OUTER_RADIUS } else { INNER_RADIUS };
        let radii = [
            Vector::new(top, top),
            Vector::new(top, top),
            Vector::new(bottom, bottom),
            Vector::new(bottom, bottom),
        ];
        canvas.draw_rrect(
            RRect::new_rect_radii(r.to_skia(), &radii),
            &paint(theme.surface_container_high),
        );

        if self.kind != Kind::Child {
            let y = match self.kind {
                Kind::Head => r.y + (r.height - ICON_SIZE) / 2.0,
                _ => r.y + ROW_PADDING,
            };
            self.draw_icon(canvas, fonts, theme, Point::new(r.x + ROW_PADDING, y));
        }

        let secondary = paint(theme.on_surface_variant);
        let primary = paint(if self.critical {
            theme.error
        } else {
            theme.on_surface
        });
        let x = r.x + TEXT_X;
        let mut y = r.y
            + match self.kind {
                Kind::Card => ROW_PADDING,
                Kind::Head => (r.height - SMALL_SIZE * LINE_HEIGHT) / 2.0,
                Kind::Child => CHILD_PADDING,
            };
        let mut line = |text: &str, size: f32, paint: &Paint| {
            fonts.draw(canvas, FONT, size, text, Point::new(x, y + size), paint);
            y += size * LINE_HEIGHT;
        };
        if self.kind != Kind::Child {
            line(&self.header, SMALL_SIZE, &secondary);
        }
        if self.kind != Kind::Head {
            line(&self.title, TEXT_SIZE, &primary);
            line(&self.text, TEXT_SIZE, &secondary);
        }

        let button = paint(theme.surface_container_highest);
        if let Some((pill, count)) = &self.pill {
            let radius = PILL_HEIGHT / 2.0;
            canvas.draw_rrect(RRect::new_rect_xy(pill.to_skia(), radius, radius), &button);
            let baseline = Point::new(
                pill.x + 10.0,
                pill.y + (PILL_HEIGHT + SMALL_SIZE) / 2.0 - 1.0,
            );
            let width = fonts.draw(
                canvas,
                FONT,
                SMALL_SIZE,
                count,
                baseline,
                &paint(theme.on_surface),
            );
            let chevron = Rect::from_xywh(
                pill.x + 10.0 + width + 2.0,
                pill.y + (PILL_HEIGHT - CHEVRON) / 2.0,
                CHEVRON,
                CHEVRON,
            );
            draw_chevron(canvas, chevron, self.kind == Kind::Head, theme);
        }
        if hovered {
            let c = self.close;
            let radius = BUTTON_SIZE / 2.0;
            canvas.draw_rrect(RRect::new_rect_xy(c.to_skia(), radius, radius), &button);
            let inset = (BUTTON_SIZE - BUTTON_ICON) / 2.0;
            Icon::Close.draw(
                canvas,
                c.inset(inset, inset).to_skia(),
                theme.on_surface_variant,
            );
        }
        canvas.restore_to_count(saved);
    }

    /// The sender's icon in a circle, or its app's initial without one.
    fn draw_icon(&self, canvas: &Canvas, fonts: &FontBook, theme: &ColorTheme, at: Point) {
        let rect = Rect::from_xywh(at.x, at.y, ICON_SIZE, ICON_SIZE);
        let circle = RRect::new_oval(rect);
        match &self.icon {
            Some(icon) => {
                let saved = canvas.save();
                canvas.clip_rrect(circle, None, true);
                canvas.draw_image_rect_with_sampling_options(
                    icon,
                    None,
                    rect,
                    SamplingOptions::new(FilterMode::Linear, MipmapMode::None),
                    &paint(theme.on_surface),
                );
                canvas.restore_to_count(saved);
            }
            None => {
                canvas.draw_rrect(circle, &paint(theme.secondary_container));
                let initial: String = self.app.chars().take(1).collect();
                let size = TITLE_SIZE;
                let width = fonts.measure(FONT, size, &initial);
                let baseline =
                    Point::new(rect.center_x() - width / 2.0, rect.center_y() + size * 0.35);
                let ink = paint(theme.on_secondary_container);
                fonts.draw(canvas, FONT, size, &initial, baseline, &ink);
            }
        }
    }
}

/// ⌄, or ⌃ when `up`.
fn draw_chevron(canvas: &Canvas, rect: Rect, up: bool, theme: &ColorTheme) {
    let (w, h) = (rect.width(), rect.height());
    let (edge, tip) = if up { (0.625, 0.375) } else { (0.375, 0.625) };
    let mut path = PathBuilder::new();
    path.move_to((rect.left + w * 0.25, rect.top + h * edge))
        .line_to((rect.left + w * 0.5, rect.top + h * tip))
        .line_to((rect.left + w * 0.75, rect.top + h * edge));
    let mut stroke = paint(theme.on_surface);
    stroke.set_style(PaintStyle::Stroke);
    stroke.set_stroke_width(2.0);
    stroke.set_stroke_cap(PaintCap::Round);
    stroke.set_stroke_join(PaintJoin::Round);
    canvas.draw_path(&path.detach(), &stroke);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Axis {
    Sideways,
    Vertical,
}

/// The left button, from its press until its release.
struct Press {
    at: (f32, f32),
    row: Option<RowKey>,
    /// Decided once the pointer leaves `DRAG_SLOP`; `None` is still a click.
    axis: Option<Axis>,
    /// How far right the row has been dragged.
    offset: f32,
    scroll: f32,
}

#[derive(Default)]
pub struct NotificationPanel {
    rows: Vec<Row>,
    /// How many notifications there are, across every row.
    total: usize,
    /// "すべて消去", for hit testing; empty without notifications.
    clear: LayoutRect,
    /// Apps whose rows are open, one per notification.
    expanded: HashSet<String>,
    scroll: f32,
    list_height: f32,
    /// The pointer, in panel coordinates, while it is over the panel.
    hover: Option<(f32, f32)>,
    press: Option<Press>,
    /// A touchpad's sideways swipe in progress, and how far right it has gone.
    swipe: Option<(RowKey, f32)>,
    /// Rows let go of: sliding back, or away with the ids in `flinging`.
    slides: HashMap<RowKey, Animation<f32>>,
    flinging: Vec<(RowKey, Vec<u32>)>,
    /// Dismissed here but maybe not yet out of the store, so kept out of view.
    gone: HashSet<u32>,
    /// Where each row is drawn, on its way to where it was last laid out.
    ys: HashMap<RowKey, Animation<f32>>,
    /// When "すべて消去" was pressed, while its rows slide away.
    clearing: Option<Instant>,
}

impl NotificationPanel {
    /// Closes every group and scrolls back to the top, for a panel opening afresh.
    pub fn reset(&mut self) {
        self.expanded.clear();
        self.scroll = 0.0;
        self.hover = None;
        self.press = None;
        self.swipe = None;
    }

    /// Lays out every row. The panel's height is always [`PANEL_HEIGHT`].
    pub fn lay_out(&mut self, store: &NotificationStore, fonts: &FontBook, now: Instant) {
        let mut unseen = store.unseen();
        self.gone.retain(|id| unseen.iter().any(|n| n.id == *id));
        unseen.retain(|n| !self.gone.contains(&n.id));
        self.total = unseen.len();
        self.rows.clear();

        let groups = group_by_app(&unseen);
        self.expanded
            .retain(|app| groups.iter().any(|g| &g[0].data.app_name == app));
        let mut y = 0.0;
        let mut push = |rows: &mut Vec<Row>, kind, n, group: &[&ActiveNotification]| {
            let row = Row::new(kind, n, group, y, now, fonts);
            y += row.rect.height + ROW_GAP;
            rows.push(row);
        };
        for group in &groups {
            if group.len() > 1 && self.expanded.contains(&group[0].data.app_name) {
                push(&mut self.rows, Kind::Head, group[0], group);
                for notification in group {
                    push(&mut self.rows, Kind::Child, notification, group);
                }
            } else {
                push(&mut self.rows, Kind::Card, group[0], group);
            }
        }
        self.list_height = (y - ROW_GAP).max(0.0) + LIST_BOTTOM;
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());

        // Rows that were already here move to their new place, from wherever they are
        // drawn now; new ones start in place.
        let mut ys = HashMap::new();
        for row in &self.rows {
            let (key, to) = (row.key(), row.rect.y);
            let y = match self.ys.remove(&key) {
                Some(mut y) if y.to() != to => {
                    y.set_target(to);
                    y
                }
                Some(y) => y,
                None => Animation::new(to, to, MOVE, Arc::new(ease_out_cubic)),
            };
            ys.insert(key, y);
        }
        self.ys = ys;

        self.clear = if unseen.is_empty() {
            LayoutRect::empty()
        } else {
            let width = fonts.measure(FONT, CLEAR_SIZE, "すべて消去") + 28.0;
            LayoutRect::new(
                PANEL_WIDTH - PANEL_PADDING - width,
                TOP_PADDING,
                width,
                HEADER_HEIGHT,
            )
        };
    }

    fn max_scroll(&self) -> f32 {
        (self.list_height - (PANEL_HEIGHT - LIST_TOP)).max(0.0)
    }

    /// The row under `x`, `y` in panel coordinates, if it is in the list's view.
    fn row_at(&self, x: f32, y: f32) -> Option<&Row> {
        if !(LIST_TOP..PANEL_HEIGHT).contains(&y) {
            return None;
        }
        let list_y = y - LIST_TOP + self.scroll;
        self.rows.iter().find(|row| row.rect.contains(x, list_y))
    }

    /// Scrolls by `dy`, and swipes the row under the pointer by `dx`. `dx` from a wheel
    /// is ignored, and `stop` ends a touchpad's swipe. Returns whether anything moved.
    pub fn on_scroll(&mut self, dx: f32, dy: f32, continuous: bool, stop: bool) -> bool {
        let scroll = (self.scroll + dy * SCROLL_SPEED).clamp(0.0, self.max_scroll());
        let mut moved = scroll != self.scroll;
        self.scroll = scroll;

        if continuous && dx != 0.0 && self.swipe.is_none() {
            let row = self.hover.and_then(|(x, y)| self.row_at(x, y));
            self.swipe = row.map(|row| (row.key(), 0.0));
        }
        if let Some((_, offset)) = &mut self.swipe {
            *offset -= dx;
            moved = true;
        }
        // ponytail: a compositor that never sends a sideways stop leaves the row out of
        // place until the next one; time the swipe out if one shows up.
        if stop {
            if let Some((key, offset)) = self.swipe.take() {
                self.let_go(key, offset);
            }
        }
        moved
    }

    /// Returns whether the pointer moved anything worth repainting.
    pub fn on_motion(&mut self, x: f32, y: f32) -> bool {
        let was = self
            .hover
            .and_then(|(x, y)| self.row_at(x, y))
            .map(Row::key);
        self.hover = Some((x, y));
        let now = self.row_at(x, y).map(Row::key);
        let max_scroll = self.max_scroll();

        let Some(press) = &mut self.press else {
            return was != now;
        };
        let (dx, dy) = (x - press.at.0, y - press.at.1);
        if press.axis.is_none() && dx.abs().max(dy.abs()) > DRAG_SLOP {
            press.axis = Some(if dx.abs() > dy.abs() && press.row.is_some() {
                Axis::Sideways
            } else {
                Axis::Vertical
            });
        }
        match press.axis {
            Some(Axis::Sideways) => press.offset = dx,
            Some(Axis::Vertical) => {
                self.scroll = (press.scroll - dy).clamp(0.0, max_scroll);
            }
            None => return was != now,
        }
        true
    }

    pub fn on_leave(&mut self) -> bool {
        let had = self.hover.take().is_some();
        if let Some(Press {
            row: Some(key),
            axis: Some(Axis::Sideways),
            offset,
            ..
        }) = self.press.take()
        {
            self.slides.insert(key, slide(offset, 0.0));
        }
        had
    }

    pub fn on_press(&mut self, x: f32, y: f32) {
        if self.clearing.is_some() {
            return;
        }
        self.press = Some(Press {
            at: (x, y),
            row: self.row_at(x, y).map(Row::key),
            axis: None,
            offset: 0.0,
            scroll: self.scroll,
        });
    }

    /// What letting go of the left button at `x`, `y` in the panel asks for. Opening or
    /// closing a group happens here, and asks only for a repaint.
    pub fn on_release(&mut self, x: f32, y: f32) -> Option<Action> {
        let press = self.press.take()?;
        match press.axis {
            Some(Axis::Sideways) => {
                self.let_go(press.row?, press.offset);
                Some(Action::Repaint)
            }
            Some(Axis::Vertical) => None,
            None => {
                let (px, py) = press.at;
                if self.clear.contains(px, py) && self.clear.contains(x, y) {
                    self.clearing = Some(Instant::now());
                    return Some(Action::Repaint);
                }
                let row = self.row_at(x, y)?;
                if press.row != Some(row.key()) {
                    return None;
                }
                let list_y = y - LIST_TOP + self.scroll;
                if row.close.contains(x, list_y) {
                    let ids = row.ids.clone();
                    self.hide(&ids);
                    return Some(Action::Dismiss(ids));
                }
                if !row.toggles() {
                    return None;
                }
                let app = row.app.clone();

                if !self.expanded.remove(&app) {
                    self.expanded.insert(app);
                }
                Some(Action::Repaint)
            }
        }
    }

    /// Slides a row let go of at `offset` away, if it went far enough, or back.
    fn let_go(&mut self, key: RowKey, offset: f32) {
        let Some(row) = self.rows.iter().find(|row| row.key() == key) else {
            return;
        };
        let (width, ids) = (row.rect.width, row.ids.clone());
        if offset.abs() > width * DISMISS_RATIO {
            let away = offset.signum() * (width + PANEL_PADDING);
            self.slides.insert(key, slide(offset, away));
            self.flinging.push((key, ids));
        } else {
            self.slides.insert(key, slide(offset, 0.0));
        }
    }

    /// The ids of rows that have finished sliding away, now to dismiss.
    pub fn take_flung(&mut self) -> Vec<u32> {
        let slides = &self.slides;
        let (done, flinging) = self
            .flinging
            .drain(..)
            .partition(|(key, _)| slides.get(key).is_none_or(|a| a.is_done()));
        self.flinging = flinging;
        self.slides.retain(|_, a| !a.is_done());
        let ids: Vec<u32> = done.into_iter().flat_map(|(_, ids)| ids).collect();
        self.hide(&ids);
        ids
    }

    /// Keeps `ids` out of view from this frame on, until the store drops them. Their rows
    /// leave now: this frame was laid out before they finished sliding, and its draw
    /// would show them back in place.
    fn hide(&mut self, ids: &[u32]) {
        self.gone.extend(ids);
        let gone = &self.gone;
        self.rows
            .retain(|row| !row.ids.iter().all(|id| gone.contains(id)));
    }

    /// Whether "すべて消去" has finished sliding every row away, and the store is to be
    /// cleared now.
    pub fn take_cleared(&mut self) -> bool {
        let Some(started) = self.clearing else {
            return false;
        };
        let visible = self.rows.iter().filter(|row| self.in_view(row)).count();
        let lasts = CLEAR_STAGGER * visible.saturating_sub(1) as u32 + SLIDE;
        if started.elapsed() < lasts {
            return false;
        }
        self.clearing = None;
        let ids: Vec<u32> = self.rows.iter().flat_map(|row| row.ids.clone()).collect();
        self.hide(&ids);
        true
    }

    fn in_view(&self, row: &Row) -> bool {
        let top = self.scroll;
        let bottom = top + PANEL_HEIGHT - LIST_TOP;
        row.rect.y < bottom && row.rect.y + row.rect.height > top
    }

    /// How far a clearing has slid `row`: each row in view starts `CLEAR_STAGGER` after
    /// the one above it.
    fn clear_offset(&self, started: Instant, row: &Row) -> f32 {
        let index = self
            .rows
            .iter()
            .take_while(|r| r.key() != row.key())
            .filter(|r| self.in_view(r))
            .count() as u32;
        let elapsed = started.elapsed().saturating_sub(CLEAR_STAGGER * index);
        let t = (elapsed.as_secs_f32() / SLIDE.as_secs_f32()).min(1.0);
        ease_out_cubic(t) * (row.rect.width + PANEL_PADDING)
    }

    fn offset(&self, row: &Row) -> f32 {
        if let Some(started) = self.clearing {
            return self.clear_offset(started, row);
        }
        let key = row.key();
        match (&self.press, &self.swipe) {
            (Some(p), _) if p.row == Some(key) && p.axis == Some(Axis::Sideways) => p.offset,
            (_, Some((k, offset))) if *k == key => *offset,
            _ => self.slides.get(&key).map_or(0.0, Animation::value),
        }
    }

    /// Returns whether a row is still sliding.
    pub fn draw(
        &self,
        canvas: &Canvas,
        size: (f32, f32),
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> bool {
        let title_baseline = TOP_PADDING + (HEADER_HEIGHT + TITLE_SIZE) / 2.0 - 2.0;
        let at = Point::new(PANEL_PADDING, title_baseline);
        fonts.draw(
            canvas,
            FONT,
            TITLE_SIZE,
            "通知",
            at,
            &paint(theme.on_surface),
        );

        if self.rows.is_empty() {
            let text = "通知はありません";
            let x = (size.0 - fonts.measure(FONT, TEXT_SIZE, text)) / 2.0;
            let y = (LIST_TOP + PANEL_HEIGHT) / 2.0;
            let ink = paint(theme.on_surface_variant);
            fonts.draw(canvas, FONT, TEXT_SIZE, text, Point::new(x, y), &ink);
            return false;
        }

        let c = self.clear;
        let radius = c.height / 2.0;
        canvas.draw_rrect(
            RRect::new_rect_xy(c.to_skia(), radius, radius),
            &paint(theme.secondary_container),
        );
        let baseline = Point::new(c.x + 14.0, c.y + (c.height + CLEAR_SIZE) / 2.0 - 1.0);
        let ink = paint(theme.on_secondary_container);
        fonts.draw(canvas, FONT, CLEAR_SIZE, "すべて消去", baseline, &ink);

        let dragging = self.press.as_ref().is_some_and(|p| p.axis.is_some());
        let hovered = self
            .hover
            .filter(|_| !dragging && self.swipe.is_none())
            .and_then(|(x, y)| self.row_at(x, y))
            .map(Row::key);

        let saved = canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(0.0, LIST_TOP, size.0, PANEL_HEIGHT - LIST_TOP),
            None,
            true,
        );
        canvas.translate((0.0, LIST_TOP - self.scroll));
        let last = self.rows.len() - 1;
        for (i, row) in self.rows.iter().enumerate() {
            let key = row.key();
            let ends = (i == 0, i == last);
            let y = self.ys.get(&key).map_or(row.rect.y, Animation::value);
            let moved = canvas.save();
            canvas.translate((0.0, y - row.rect.y));
            row.draw(
                canvas,
                fonts,
                theme,
                ends,
                hovered == Some(key),
                self.offset(row),
            );
            canvas.restore_to_count(moved);
        }
        canvas.restore_to_count(saved);

        self.slides.values().any(Animation::is_traveling)
            || self.ys.values().any(Animation::is_traveling)
            || !self.flinging.is_empty()
            || self.clearing.is_some()
    }
}

fn slide(from: f32, to: f32) -> Animation<f32> {
    Animation::new(from, to, SLIDE, Arc::new(ease_out_cubic))
}

#[cfg(test)]
mod tests {
    use skia_safe::FontStyle;

    use super::*;
    use crate::dbus::notification::{NotificationEvent, NotificationSnapshot};
    use crate::ui::ISLAND_MAX_HEIGHT;

    #[test]
    fn ago_counts_minutes_hours_and_days() {
        let min = |m: u64| Duration::from_secs(m * 60);
        assert_eq!(ago(Duration::from_secs(59)), "たった今");
        assert_eq!(ago(min(5)), "5分前");
        assert_eq!(ago(min(60)), "1時間前");
        assert_eq!(ago(min(60 * 24 * 2)), "2日前");
    }

    #[test]
    fn three_cards_fit_the_headroom() {
        assert!(PANEL_HEIGHT <= ISLAND_MAX_HEIGHT as f32);
    }

    fn posted(apps: &[&str]) -> NotificationStore {
        let now = Instant::now();
        let mut store = NotificationStore::default();
        for (id, app) in (1..).zip(apps) {
            let data = NotificationSnapshot {
                app_name: app.to_string(),
                summary: "summary".into(),
                expire_timeout: 0,
                ..Default::default()
            };
            store.apply(&NotificationEvent::Posted { id, data }, now);
        }
        store
    }

    fn panel(store: &NotificationStore) -> (NotificationPanel, FontBook) {
        let mut fonts = FontBook::new();
        fonts.register(FONT, "Noto Sans CJK JP", FontStyle::normal());
        let mut panel = NotificationPanel::default();
        panel.lay_out(store, &fonts, Instant::now());
        (panel, fonts)
    }

    /// The middle of `row`'s text, in panel coordinates.
    fn body_of(panel: &NotificationPanel, row: usize) -> (f32, f32) {
        let r = panel.rows[row].rect;
        (
            r.x + TEXT_X + 10.0,
            r.y + LIST_TOP - panel.scroll + r.height / 2.0,
        )
    }

    fn click(panel: &mut NotificationPanel, (x, y): (f32, f32)) -> Option<Action> {
        panel.on_motion(x, y);
        panel.on_press(x, y);
        panel.on_release(x, y)
    }

    #[test]
    fn an_app_stacks_into_one_row_that_clicks_open() {
        let store = posted(&["a", "b", "a", "a"]);
        let (mut panel, fonts) = panel(&store);
        let now = Instant::now();

        assert_eq!(panel.rows.len(), 2);
        assert_eq!(
            panel.rows[0].ids,
            [4, 3, 1],
            "the newest app first, newest on top"
        );
        assert!(panel.rows[0].pill.is_some() && panel.rows[1].pill.is_none());

        assert!(matches!(
            {
                let at = body_of(&panel, 0);
                click(&mut panel, at)
            },
            Some(Action::Repaint)
        ));
        panel.lay_out(&store, &fonts, now);
        let kinds: Vec<Kind> = panel.rows.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            [
                Kind::Head,
                Kind::Child,
                Kind::Child,
                Kind::Child,
                Kind::Card
            ]
        );
        assert_eq!(panel.rows[2].ids, [3], "open, each dismisses alone");

        assert!(
            {
                let at = body_of(&panel, 2);
                click(&mut panel, at)
            }
            .is_none(),
            "a child doesn't close it"
        );
        assert!(matches!(
            {
                let at = body_of(&panel, 0);
                click(&mut panel, at)
            },
            Some(Action::Repaint)
        ));
        panel.lay_out(&store, &fonts, now);
        assert_eq!(panel.rows.len(), 2, "its heading closes it");

        let close = panel.rows[0].close;
        let at = (close.x + 2.0, close.y + LIST_TOP + 2.0);
        assert!(matches!(click(&mut panel, at), Some(Action::Dismiss(ids)) if ids == [4, 3, 1]));
        panel.lay_out(&store, &fonts, now);
        assert_eq!(
            panel.rows.len(),
            1,
            "gone at once, before the store hears of it"
        );
    }

    #[test]
    fn dragging_far_enough_sideways_slides_a_row_away() {
        let store = posted(&["a", "b"]);
        let (mut panel, _) = panel(&store);
        let (x, y) = body_of(&panel, 1);

        panel.on_press(x, y);
        assert!(panel.on_motion(x + 30.0, y + 2.0));
        assert_eq!(panel.offset(&panel.rows[1]), 30.0);
        assert!(matches!(
            panel.on_release(x + 30.0, y),
            Some(Action::Repaint)
        ));
        assert!(panel.flinging.is_empty(), "too short: it slides back");

        panel.on_press(x, y);
        panel.on_motion(x - 200.0, y);
        panel.on_release(x - 200.0, y);
        assert_eq!(panel.flinging.len(), 1);
        assert!(panel.take_flung().is_empty(), "still sliding");
        std::thread::sleep(SLIDE + Duration::from_millis(20));
        assert_eq!(panel.take_flung(), [1]);
        assert_eq!(
            panel.rows.len(),
            1,
            "out of this frame's draw, not back in place"
        );
    }

    #[test]
    fn a_touchpad_swipe_slides_the_row_under_the_pointer() {
        let store = posted(&["a", "b"]);
        let (mut panel, _) = panel(&store);
        let (x, y) = body_of(&panel, 0);
        panel.on_motion(x, y);

        assert!(
            !panel.on_scroll(-40.0, 0.0, false, false),
            "a wheel doesn't swipe"
        );
        assert!(panel.on_scroll(-80.0, 0.0, true, false));
        assert!(panel.on_scroll(-80.0, 0.0, true, false));
        assert_eq!(panel.offset(&panel.rows[0]), 160.0);
        panel.on_scroll(0.0, 0.0, true, true);
        assert_eq!(panel.flinging.len(), 1);
        assert_eq!(panel.flinging[0].1, [2]);
    }

    #[test]
    fn the_list_scrolls_by_wheel_or_vertical_drag_within_its_bounds() {
        let apps: Vec<String> = (0..6).map(|i| i.to_string()).collect();
        let apps: Vec<&str> = apps.iter().map(String::as_str).collect();
        let store = posted(&apps);
        let (mut panel, fonts) = panel(&store);
        let max = 6.0 * (CARD_HEIGHT + ROW_GAP) - ROW_GAP + LIST_BOTTOM - (PANEL_HEIGHT - LIST_TOP);

        assert!(
            !panel.on_scroll(0.0, -10.0, true, false),
            "already at the top"
        );
        assert!(panel.on_scroll(0.0, 10_000.0, true, false));
        assert_eq!(panel.scroll, max);
        let (x, y) = body_of(&panel, 5);
        assert!(y < PANEL_HEIGHT, "the last row is in view");
        let id = panel.rows[5].ids[0];
        let close = panel.rows[5].close;
        let at = (close.x + 2.0, close.y + LIST_TOP - panel.scroll + 2.0);
        assert!(matches!(click(&mut panel, at), Some(Action::Dismiss(ids)) if ids == [id]));
        assert!(
            click(&mut panel, (x, LIST_TOP - 1.0)).is_none(),
            "the header doesn't scroll"
        );

        panel.scroll = 0.0;
        let (x, y) = body_of(&panel, 1);
        panel.on_press(x, y);
        panel.on_motion(x, y - 50.0);
        assert_eq!(panel.scroll, 50.0, "dragging up scrolls down");
        assert!(panel.on_release(x, y - 50.0).is_none());

        panel.lay_out(&posted(&apps[..2]), &fonts, Instant::now());
        assert_eq!(panel.scroll, 0.0, "shrinking clamps it");
    }

    #[test]
    fn the_rows_below_a_dismissed_one_move_up_into_its_place() {
        let store = posted(&["a", "b", "c"]);
        let (mut panel, fonts) = panel(&store);
        let below = panel.rows[2].key();
        let was = panel.rows[2].rect.y;

        let close = panel.rows[1].close;
        let at = (close.x + 2.0, close.y + LIST_TOP + 2.0);
        assert!(matches!(click(&mut panel, at), Some(Action::Dismiss(_))));
        panel.lay_out(&store, &fonts, Instant::now());

        let to = panel.rows[1].rect.y;
        assert!(to < was);
        assert!(
            panel.ys[&below].value() > was - 1.0,
            "it starts where it was"
        );
        std::thread::sleep(MOVE + Duration::from_millis(20));
        assert_eq!(panel.ys[&below].value(), to);
    }

    #[test]
    fn clearing_slides_the_rows_away_from_the_top_then_clears() {
        let store = posted(&["a", "b", "c"]);
        let (mut panel, fonts) = panel(&store);
        let clear = panel.clear;
        let at = (clear.x + 2.0, clear.y + 2.0);
        assert!(matches!(click(&mut panel, at), Some(Action::Repaint)));
        assert!(!panel.take_cleared(), "not before they have gone");

        std::thread::sleep(CLEAR_STAGGER + Duration::from_millis(20));
        let offsets: Vec<f32> = panel.rows.iter().map(|row| panel.offset(row)).collect();
        assert!(offsets[0] > offsets[1] && offsets[1] > 0.0 && offsets[2] == 0.0);

        std::thread::sleep(CLEAR_STAGGER * 2 + SLIDE);
        assert!(panel.take_cleared());
        assert!(panel.rows.is_empty(), "out of this frame's draw too");
        panel.lay_out(&store, &fonts, Instant::now());
        assert!(panel.rows.is_empty(), "hidden until the store has cleared");
    }
}
