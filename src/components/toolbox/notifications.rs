//! The toolbox's other panel, opened by a right click on its row: the notifications nobody has
//! dismissed yet, including the ones that expired unseen. Laid out like Android's shade: one
//! rounded section of rows, each app's notifications stacked into one row until it is clicked
//! open. Rows go by the ✕ that shows on hover, or all at once by "すべて消去". The list scrolls in
//! a panel three rows tall.

use std::{
    cell::Cell,
    collections::HashSet,
    rc::Rc,
    time::{Duration, Instant},
};

use anti_webui::prelude::*;
use anti_webui::shell::prelude::{local_time, Every};
use anti_webui::style::{BackgroundSize, ImageSource, LineHeight};
use anti_webui_md3e::{components::sized_icon, icons};
use ui_core::util::elide;

use super::{PANEL_PADDING, PANEL_WIDTH};
use crate::{
    dbus::notification::{ActiveNotification, Urgency},
    state::State,
    ui::{measure, palette},
};

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
const BUTTON_ICON: f32 = 20.0;
const PILL_HEIGHT: f32 = 24.0;
const CHEVRON: f32 = 16.0;
const LIST_TOP: f32 = TOP_PADDING + HEADER_HEIGHT + HEADER_GAP;
/// Below the last row, inside the scrolling list.
const LIST_BOTTOM: f32 = 12.0;
const ROW_WIDTH: f32 = PANEL_WIDTH - PANEL_PADDING * 2.0;
/// How much of its width a row must be dragged to go once let go.
const DISMISS_RATIO: f32 = 0.4;
/// Where a row slides away to: out past the panel's edge.
const GONE: f32 = ROW_WIDTH + PANEL_PADDING;
const SLIDE: Transition = Transition::ms(200).cubic_bezier(0.33, 1.0, 0.68, 1.0);
/// How long rows take to close a gap, or to make room for an app opening.
const MOVE: Transition = Transition::ms(250).cubic_bezier(0.33, 1.0, 0.68, 1.0);
/// How far below its place a row coming in starts.
const ENTER_RISE: f32 = 16.0;
/// "すべて消去" slides the rows away one after another, from the top, this far apart.
const CLEAR_STAGGER: Duration = Duration::from_millis(50);
/// Past this many, the rows go together: the panel shows about three.
const CLEAR_STAGGERED: usize = 3;
/// Always three cards tall, whatever there is.
pub const PANEL_HEIGHT: f32 = LIST_TOP + CARD_HEIGHT * 3.0 + ROW_GAP * 2.0 + LIST_BOTTOM;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    /// A notification, or with more than one in its group an app's, stacked.
    Card,
    /// An open app's heading, with its notifications as `Child`ren below.
    Head,
    Child,
}

/// Which row, across updates: a group's heading and its first notification share an id.
type RowKey = (u32, Kind);

/// One row, as it is shown.
#[derive(Clone, PartialEq)]
struct Row {
    kind: Kind,
    /// What dismissing it dismisses: one, or a stacked app's.
    ids: Vec<u32>,
    app: String,
    group_len: usize,
    critical: bool,
    icon: Option<ImageSource>,
    header: String,
    title: String,
    text: String,
}

impl Row {
    fn new(kind: Kind, notification: &ActiveNotification, group: &[&ActiveNotification], now: Instant) -> Self {
        let data = &notification.data;
        let ago = ago(now.saturating_duration_since(notification.posted));
        let (header, title) = match kind {
            Kind::Card => (format!("{} · {ago}", data.app_name), data.summary.clone()),
            Kind::Head => (format!("{} · {}件", data.app_name, group.len()), String::new()),
            Kind::Child => (String::new(), format!("{} · {ago}", data.summary)),
        };
        Self {
            kind,
            ids: if kind == Kind::Child { vec![notification.id] } else { group.iter().map(|n| n.id).collect() },
            app: data.app_name.clone(),
            group_len: group.len(),
            critical: data.urgency == Urgency::Critical,
            icon: data.icon.clone(),
            header,
            title,
            text: data.body.lines().next().unwrap_or_default().to_string(),
        }
    }

    fn key(&self) -> RowKey {
        (self.ids[0], self.kind)
    }

    /// Whether clicking it opens or closes its app.
    fn toggles(&self) -> bool {
        self.group_len > 1 && self.kind != Kind::Child
    }

    fn pill(&self) -> Option<String> {
        (self.kind == Kind::Head || (self.kind == Kind::Card && self.group_len > 1)).then(|| self.group_len.to_string())
    }
}

/// Every row, in order: an open app's heading and its notifications, or one row per app.
fn rows(unseen: &[&ActiveNotification], expanded: &HashSet<String>, now: Instant) -> Vec<Row> {
    let mut rows = Vec::new();
    for group in group_by_app(unseen) {
        if group.len() > 1 && expanded.contains(&group[0].data.app_name) {
            rows.push(Row::new(Kind::Head, group[0], &group, now));
            rows.extend(group.iter().map(|n| Row::new(Kind::Child, n, &group, now)));
        } else {
            rows.push(Row::new(Kind::Card, group[0], &group, now));
        }
    }
    rows
}

/// The panel, PANEL_WIDTH by PANEL_HEIGHT. `expanded` holds the apps whose rows are open.
pub fn panel(state: &State, expanded: Signal<HashSet<String>>) -> Div {
    let notifications = state.notifications;
    let colors = state.clone();
    let color = move |pick: Pick| palette(&colors, pick);
    let color = Rc::new(color);
    let rows = memo(move || {
        // Again every minute too, for the "5分前".
        local_time(Every::Minute);
        notifications.with(|n| rows(&n.unseen(), &expanded.get(), Instant::now()))
    });
    // "すべて消去" pressed: the rows are sliding away, and the store clears once they have.
    let clearing = signal(false);

    let header = {
        let (state, c1, c2) = (state.clone(), color.clone(), color.clone());
        let clear_width = measure(CLEAR_SIZE)("すべて消去") + 28.0;
        div()
            .h(HEADER_HEIGHT)
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(text("通知").text_size(TITLE_SIZE).dyn_style(move |s| s.text_color(c1(|t| t.on_surface))))
            .child(
                div()
                    .w(clear_width)
                    .h(HEADER_HEIGHT)
                    .rounded(HEADER_HEIGHT / 2.0)
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor(CursorIcon::Pointer)
                    .dyn_style(move |s| {
                        let s = s
                            .bg(c2(|t| t.secondary_container))
                            .text_color(c2(|t| t.on_secondary_container));
                        if rows.with(Vec::is_empty) { s.hidden() } else { s }
                    })
                    .on_click(move |_| {
                        if clearing.get_untracked() {
                            return;
                        }
                        clearing.set(true);
                        let state = state.clone();
                        set_timeout(clear_duration(untrack(|| rows.with(Vec::len))), move || {
                            state.clear_notifications();
                            clearing.set(false);
                        });
                    })
                    .child(text("すべて消去").text_size(CLEAR_SIZE)),
            )
    };

    let empty = {
        let c = color.clone();
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .dyn_style(move |s| if rows.with(Vec::is_empty) { s } else { s.hidden() })
            .child(text("通知はありません").text_size(TEXT_SIZE).dyn_style(move |s| s.text_color(c(|t| t.on_surface_variant))))
    };

    let state = state.clone();
    let list = div()
        .flex_1()
        .min_h(0.0)
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(ROW_GAP)
        .pb(LIST_BOTTOM)
        .dyn_style(move |s| if rows.with(Vec::is_empty) { s.hidden() } else { s })
        .child(
            for_each(
                move || rows.get(),
                Row::key,
                move |row| row_view(&state, row.key(), rows, expanded, clearing, color.clone()),
            )
            // Rows close a gap, make room for an app opening, and an opened app's
            // notifications come out from under its heading.
            .moves(MOVE)
            .enter(|s| s.opacity(0.0).translate(0.0, -ENTER_RISE))
            .exit(|s| s.opacity(0.0))
            .transition(MOVE),
        );

    div()
        .w(PANEL_WIDTH)
        .h(PANEL_HEIGHT)
        .px(PANEL_PADDING)
        .pt(TOP_PADDING)
        .flex()
        .flex_col()
        .gap(HEADER_GAP)
        .child(header)
        .child(empty)
        .child(list)
}

type Pick = fn(&ui_core::scheme::ColorTheme) -> ui_core::scheme::color::Color;

/// How long "すべて消去" takes to slide `rows` away: each starts `CLEAR_STAGGER` after the one
/// above it, up to the rows the panel shows.
fn clear_duration(rows: usize) -> Duration {
    CLEAR_STAGGER * rows.saturating_sub(1).min(CLEAR_STAGGERED) as u32 + SLIDE.duration
}

/// Where a row goes when it is let go of `offset` from its place: away to that side, or back.
fn let_go(offset: f32) -> f32 {
    if offset.abs() > ROW_WIDTH * DISMISS_RATIO { offset.signum() * GONE } else { 0.0 }
}

/// A row: its shape, and what moves it sideways. What is in it follows `rows`, so that it stays
/// the same row while that changes, like its "5分前".
fn row_view(
    state: &State,
    key: RowKey,
    rows: Memo<Vec<Row>>,
    expanded: Signal<HashSet<String>>,
    clearing: Signal<bool>,
    color: Rc<impl Fn(Pick) -> Color + 'static>,
) -> Div {
    let current = move || rows.with(|rs| rs.iter().find(|r| r.key() == key).cloned());
    let hovered = signal(false);
    let offset = animated(0.0f32);
    let sideways = Rc::new(Cell::new(false));
    let height = match key.1 {
        Kind::Card => CARD_HEIGHT,
        Kind::Head => HEAD_HEIGHT,
        Kind::Child => CHILD_HEIGHT,
    };
    // Slides away to `to`, then dismisses what the row holds.
    let slide_away = {
        let state = state.clone();
        Rc::new(move |to: f32| {
            offset.to(to, SLIDE);
            let ids = untrack(current).map(|r| r.ids).unwrap_or_default();
            let state = state.clone();
            set_timeout(SLIDE.duration, move || {
                for id in &ids {
                    state.dismiss_notification(*id);
                }
            });
        })
    };

    // The store clears them all at the end; this only moves the row.
    effect(move || {
        if !clearing.get() {
            return;
        }
        let index = untrack(|| rows.with(|rs| rs.iter().position(|r| r.key() == key))).unwrap_or(0);
        set_timeout(CLEAR_STAGGER * index.min(CLEAR_STAGGERED) as u32, move || offset.to(GONE, SLIDE));
    });

    let content = {
        let (color, slide_away) = (color.clone(), slide_away.clone());
        dyn_view(move || match current() {
            Some(row) => row_content(row, height, hovered, color.clone(), slide_away.clone()).into_view(),
            None => div().into_view(),
        })
    };

    let toggle = move |_: &PointerEvent| {
        let Some(row) = untrack(current) else { return };
        if row.toggles() {
            expanded.update(|e| {
                if !e.remove(&row.app) {
                    e.insert(row.app.clone());
                }
            });
        }
    };

    div()
        .relative()
        .w_full()
        .h(height)
        .flex_none()
        .on_hover(move |h| hovered.set(h))
        .dyn_style({
            let c = color.clone();
            move |s| {
                let (first, last) = rows.with(|rs| {
                    (rs.first().is_some_and(|r| r.key() == key), rs.last().is_some_and(|r| r.key() == key))
                });
                let toggles = current().is_some_and(|r| r.toggles());
                let x = offset.get();
                let s = s
                    .bg(c(|t| t.surface_container_high))
                    .rounded_t(if first { OUTER_RADIUS } else { INNER_RADIUS })
                    .rounded_b(if last { OUTER_RADIUS } else { INNER_RADIUS })
                    .translate(x, 0.0)
                    .opacity(1.0 - (x.abs() / ROW_WIDTH).min(1.0) * 0.8);
                if toggles { s.cursor(CursorIcon::Pointer) } else { s }
            }
        })
        // Dragged sideways with the mouse, it follows; let go far enough and it goes.
        .on_grab(move |e| match e.phase {
            GrabPhase::Start => sideways.set(e.total.x.abs() > e.total.y.abs()),
            GrabPhase::Move if sideways.get() => offset.set(e.total.x),
            GrabPhase::End if sideways.get() => match let_go(offset.get_untracked()) {
                0.0 => offset.to(0.0, SLIDE),
                to => slide_away(to),
            },
            GrabPhase::Cancel => offset.to(0.0, SLIDE),
            _ => {}
        })
        .on_click(toggle)
        .child(content)
}

/// What is in a row: the icon, the lines, and the ✕ and the count.
fn row_content(
    row: Row,
    height: f32,
    hovered: Signal<bool>,
    color: Rc<impl Fn(Pick) -> Color + 'static>,
    slide_away: Rc<dyn Fn(f32)>,
) -> Div {
    let pill = row.pill();
    // The ✕ only shows on hover, but the text stops short of it anyway, so that hovering
    // doesn't reflow it.
    let pill_width = pill.as_ref().map_or(0.0, |count| 10.0 + measure(SMALL_SIZE)(count) + 2.0 + CHEVRON + 6.0 + 4.0);
    let text_width = ROW_WIDTH - TEXT_X - ROW_PADDING - pill_width - BUTTON_SIZE - 8.0;

    let line = |content: &str, size: f32, pick: Pick| {
        let color = color.clone();
        text(elide(content, text_width, measure(size)))
            .text_size(size)
            .line_height(LineHeight::Relative(LINE_HEIGHT))
            .whitespace_nowrap()
            .dyn_style(move |s| s.text_color(color(pick)))
    };
    let primary: Pick = if row.critical { |t| t.error } else { |t| t.on_surface };
    let mut lines = div().flex().flex_col();
    if row.kind != Kind::Child {
        lines = lines.child(line(&row.header, SMALL_SIZE, |t| t.on_surface_variant));
    }
    if row.kind != Kind::Head {
        lines = lines.child(line(&row.title, TEXT_SIZE, primary)).child(line(&row.text, TEXT_SIZE, |t| t.on_surface_variant));
    }

    let button_bg = color.clone();
    let mut controls = div().flex().flex_row().items_center().gap(4.0).child(
        div()
            .size(BUTTON_SIZE)
            .rounded(BUTTON_SIZE / 2.0)
            .flex()
            .items_center()
            .justify_center()
            .cursor(CursorIcon::Pointer)
            .dyn_style(move |s| {
                let s = s.bg(button_bg(|t| t.surface_container_highest)).text_color(button_bg(|t| t.on_surface_variant));
                if hovered.get() { s } else { s.opacity(0.0) }
            })
            .on_click(move |e| {
                e.stop_propagation();
                slide_away(GONE);
            })
            .child(sized_icon(icons::CLOSE, BUTTON_ICON)),
    );
    if let Some(count) = pill {
        let c = color.clone();
        let chevron = if row.kind == Kind::Head { icons::EXPAND_LESS } else { icons::EXPAND_MORE };
        controls = controls.child(
            div()
                .h(PILL_HEIGHT)
                .pl(10.0)
                .pr(6.0)
                .gap(2.0)
                .rounded(PILL_HEIGHT / 2.0)
                .flex()
                .flex_row()
                .items_center()
                .dyn_style(move |s| s.bg(c(|t| t.surface_container_highest)).text_color(c(|t| t.on_surface)))
                .child(text(count).text_size(SMALL_SIZE))
                .child(sized_icon(chevron, CHEVRON)),
        );
    }

    let top = if row.kind == Kind::Child { CHILD_PADDING } else { ROW_PADDING };
    let mut view = div().absolute().left(0.0).top(0.0).size_full();
    if row.kind != Kind::Child {
        let icon_top = if row.kind == Kind::Head { (height - ICON_SIZE) / 2.0 } else { ROW_PADDING };
        view = view.child(icon(&row, color.clone()).absolute().left(ROW_PADDING).top(icon_top));
    }
    let lines_top = if row.kind == Kind::Head { (height - SMALL_SIZE * LINE_HEIGHT) / 2.0 } else { top };
    view.child(lines.absolute().left(TEXT_X).top(lines_top))
        .child(controls.absolute().right(ROW_PADDING).top(ROW_PADDING))
}

/// The sender's icon in a circle, or its app's initial without one.
fn icon(row: &Row, color: Rc<impl Fn(Pick) -> Color + 'static>) -> Div {
    let circle = div().size(ICON_SIZE).rounded(ICON_SIZE / 2.0).overflow_hidden();
    match &row.icon {
        Some(icon) => circle.bg_image(icon.clone()).bg_size(BackgroundSize::Cover),
        None => circle
            .flex()
            .items_center()
            .justify_center()
            .dyn_style(move |s| s.bg(color(|t| t.secondary_container)).text_color(color(|t| t.on_secondary_container)))
            .child(text(row.app.chars().take(1).collect::<String>()).text_size(TITLE_SIZE)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::notification::{NotificationEvent, NotificationSnapshot, NotificationStore};

    #[test]
    fn ago_counts_minutes_hours_and_days() {
        assert_eq!(ago(Duration::from_secs(30)), "たった今");
        assert_eq!(ago(Duration::from_secs(5 * 60)), "5分前");
        assert_eq!(ago(Duration::from_secs(3 * 3600)), "3時間前");
        assert_eq!(ago(Duration::from_secs(2 * 86400)), "2日前");
    }

    #[test]
    fn a_row_let_go_far_enough_goes_to_that_side_and_otherwise_comes_back() {
        let far = ROW_WIDTH * DISMISS_RATIO + 1.0;
        assert_eq!(let_go(far), GONE);
        assert_eq!(let_go(-far), -GONE);
        assert_eq!(let_go(ROW_WIDTH * DISMISS_RATIO - 1.0), 0.0);
        assert_eq!(let_go(0.0), 0.0);
    }

    #[test]
    fn clearing_staggers_only_the_rows_in_view() {
        assert_eq!(clear_duration(1), SLIDE.duration);
        assert_eq!(clear_duration(3), CLEAR_STAGGER * 2 + SLIDE.duration);
        assert_eq!(clear_duration(50), CLEAR_STAGGER * CLEAR_STAGGERED as u32 + SLIDE.duration);
    }

    #[test]
    fn three_cards_fit_the_headroom() {
        assert!(PANEL_HEIGHT <= crate::ui::ISLAND_MAX_HEIGHT);
    }

    #[test]
    fn an_app_stacks_into_one_row_that_opens_into_a_heading_and_its_notifications() {
        let mut store = NotificationStore::default();
        let now = Instant::now();
        for (id, app) in [(1, "chat"), (2, "mail"), (3, "chat")] {
            let data = NotificationSnapshot { app_name: app.into(), summary: format!("s{id}"), expire_timeout: 0, ..Default::default() };
            store.apply(&NotificationEvent::Posted { id, data }, now);
        }
        let unseen = store.unseen();

        let closed = rows(&unseen, &HashSet::new(), now);
        assert_eq!(closed.len(), 2, "one row per app");
        let chat = closed.iter().find(|r| r.app == "chat").unwrap();
        assert!(chat.toggles() && chat.pill().as_deref() == Some("2"));
        assert_eq!(chat.ids.len(), 2, "dismissing the stack dismisses both");
        let mail = closed.iter().find(|r| r.app == "mail").unwrap();
        assert!(!mail.toggles() && mail.pill().is_none());

        let open = rows(&unseen, &HashSet::from(["chat".to_string()]), now);
        let kinds: Vec<_> = open.iter().filter(|r| r.app == "chat").map(|r| r.kind).collect();
        assert_eq!(kinds, [Kind::Head, Kind::Child, Kind::Child]);
        assert!(open.iter().filter(|r| r.kind == Kind::Child).all(|r| r.ids.len() == 1 && !r.toggles()));
    }
}
