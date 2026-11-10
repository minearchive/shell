//! The bar's status icons, and the panel of details they open. A right click opens the
//! notifications nobody has dismissed instead.

mod items;
mod notifications;

use std::{collections::HashSet, rc::Rc};

use anti_webui::prelude::*;
use anti_webui_md3e::{components::sized_icon, icons};
use ui_core::geometry::LayoutRect;

use self::items::{items, DetailWidth, ICON_SIZE};
use crate::{
    state::State,
    ui::{measure, palette, BarSlot, IslandPart, IslandPlacement, IslandRequest, Part, BAR_HEIGHT},
};

const ICON_GAP: f32 = 14.0;
const ROW_PADDING: f32 = 20.0;

pub(super) const PANEL_WIDTH: f32 = 380.0;
pub(super) const PANEL_PADDING: f32 = 16.0;
const PANEL_GAP: f32 = 12.0;
/// Above a normal notification, so one can't pull the panel away from under a drag.
const PANEL_PRIORITY: u8 = 150;
const BADGE_HEIGHT: f32 = 16.0;
const BADGE_TEXT: f32 = 11.0;

/// Which panel the row opens: a left click opens the controls, a right click the
/// notifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Controls,
    Notifications,
}

/// Where each entry goes in the panel, and the panel's height: captions on top, then
/// half-width entries in pairs, then full-width entries. Zero-height entries take no
/// space. No padding below the last entry: the bar's row continues the panel there.
fn panel_layout(entries: &[(DetailWidth, f32)], panel_width: f32) -> (Vec<LayoutRect>, f32) {
    let inner_width = panel_width - PANEL_PADDING * 2.0;
    let half_width = (inner_width - PANEL_GAP) / 2.0;
    let mut rects = vec![LayoutRect::empty(); entries.len()];
    let mut y = PANEL_PADDING;

    for (i, entry) in entries.iter().enumerate() {
        if entry.0 == DetailWidth::Caption {
            rects[i] = LayoutRect::new(PANEL_PADDING, y, inner_width, entry.1);
            if entry.1 > 0.0 {
                y += entry.1 + PANEL_GAP;
            }
        }
    }

    let halves: Vec<usize> = (0..entries.len())
        .filter(|&i| entries[i].0 == DetailWidth::Half)
        .collect();
    for row in halves.chunks(2) {
        let mut row_height: f32 = 0.0;
        for (column, &i) in row.iter().enumerate() {
            let x = PANEL_PADDING + column as f32 * (half_width + PANEL_GAP);
            rects[i] = LayoutRect::new(x, y, half_width, entries[i].1);
            row_height = row_height.max(entries[i].1);
        }
        y += row_height + PANEL_GAP;
    }

    for (i, entry) in entries.iter().enumerate() {
        if entry.0 == DetailWidth::Full {
            rects[i] = LayoutRect::new(PANEL_PADDING, y, inner_width, entry.1);
            y += entry.1 + PANEL_GAP;
        }
    }

    (rects, (y - PANEL_GAP).max(PANEL_PADDING))
}

/// A symbol, compared by which one it is.
#[derive(Clone, Copy)]
struct Glyph(&'static icons::Symbol);

impl PartialEq for Glyph {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

fn row_width(icons: usize) -> f32 {
    let icons = icons as f32;
    ROW_PADDING * 2.0 + icons * ICON_SIZE + (icons - 1.0).max(0.0) * ICON_GAP
}

pub fn toolbox(state: &State) -> Part {
    let system = state.system;
    let (items, views): (Vec<_>, Vec<_>) = items(state).into_iter().map(|i| ((i.width, i.height, i.bar_icon), i.view)).unzip();
    let items: Rc<[_]> = items.into();
    let open = signal(None::<Panel>);
    // The panel last opened, still shown while the island collapses.
    let shown = signal(Panel::Controls);
    let expanded = signal(HashSet::new());
    effect(move || {
        if let Some(panel) = open.get() {
            untrack(|| {
                shown.set(panel);
                // A panel opening afresh starts with every app closed.
                if panel == Panel::Notifications {
                    expanded.set(HashSet::new());
                }
            });
        }
    });

    let layout = {
        let items = items.clone();
        memo(move || {
            let entries: Vec<_> = system.with(|s| items.iter().map(|(width, height, _)| (*width, height(s))).collect());
            panel_layout(&entries, PANEL_WIDTH)
        })
    };
    let bar_icons = {
        let items = items.clone();
        memo(move || system.with(|s| items.iter().filter_map(|(_, _, bar_icon)| bar_icon(s)).map(Glyph).collect::<Vec<_>>()))
    };
    let request = memo(move || {
        let height = match open.get()? {
            Panel::Controls => layout.with(|l| l.1),
            Panel::Notifications => notifications::PANEL_HEIGHT,
        };
        Some(IslandRequest {
            width: PANEL_WIDTH,
            height,
            placement: IslandPlacement::Corner { from_width: bar_icons.with(|i| row_width(i.len())) },
            priority: PANEL_PRIORITY,
        })
    });

    Part {
        bar: Some((BarSlot::Right, row(state, bar_icons, open).into_view())),
        island: Some(IslandPart { request, content: div()
                .relative()
                .child(panel(views, layout).dyn_style(move |s| if shown.get() == Panel::Controls { s } else { s.hidden() }))
                .child(
                    notifications::panel(state, expanded)
                        .dyn_style(move |s| if shown.get() == Panel::Notifications { s } else { s.hidden() }),
                )
                .into_view(),
            close: Rc::new(move || open.set(None)) }),
    }
}

/// The icons, or a close button while a panel is open. A left click toggles the controls, a
/// right click the notifications, and either switches from the other panel.
fn row(state: &State, bar_icons: Memo<Vec<Glyph>>, open: Signal<Option<Panel>>) -> Div {
    let toggle = move |panel| open.update(|o| *o = (*o != Some(panel)).then_some(panel));
    let on_surface = {
        let state = state.clone();
        move || palette(&state, |t| t.on_surface)
    };
    let notifications = state.notifications;
    let badge_colors = state.clone();

    div()
        .relative()
        .h(BAR_HEIGHT)
        .px(ROW_PADDING)
        .flex()
        .flex_row()
        .items_center()
        .gap(ICON_GAP)
        .cursor(CursorIcon::Pointer)
        .dyn_style(move |s| s.text_color(on_surface()))
        .on_click(move |_| toggle(Panel::Controls))
        .on_pointer_down(move |e| {
            if e.button == Some(PointerButton::Secondary) {
                toggle(Panel::Notifications);
            }
        })
        .child(dyn_view(move || {
            if open.get().is_some() {
                let count = bar_icons.with(Vec::len);
                // Keeps the row's width, so the island's corner doesn't jump.
                return div()
                    .w(row_width(count) - ROW_PADDING * 2.0)
                    .flex()
                    .justify_center()
                    .child(sized_icon(icons::CLOSE, ICON_SIZE))
                    .into_view();
            }
            let mut icons_row = div().flex().flex_row().gap(ICON_GAP);
            for Glyph(symbol) in bar_icons.get() {
                icons_row = icons_row.child(sized_icon(symbol, ICON_SIZE));
            }
            icons_row.into_view()
        }))
        // A right click is easy to miss, so say when it has something to show.
        .child(dyn_view(move || {
            let unseen = notifications.with(|n| n.unseen_len());
            if unseen == 0 || open.get().is_some() {
                return div().into_view();
            }
            let label = unseen.to_string();
            let width = (measure(BADGE_TEXT)(&label) + 8.0).max(BADGE_HEIGHT);
            let colors = badge_colors.clone();
            div()
                .absolute()
                .right(ROW_PADDING - BADGE_HEIGHT / 4.0)
                .top((BAR_HEIGHT - ICON_SIZE) / 2.0 - BADGE_HEIGHT / 2.0)
                .w(width)
                .h(BADGE_HEIGHT)
                .rounded(BADGE_HEIGHT / 2.0)
                .flex()
                .items_center()
                .justify_center()
                .dyn_style(move |s| {
                    s.bg(palette(&colors, |t| t.primary)).text_color(palette(&colors, |t| t.on_primary))
                })
                .child(text(label).text_size(BADGE_TEXT))
                .into_view()
        }))
}

/// The controls, each entry at its place from [`panel_layout`].
fn panel(views: Vec<View>, layout: Memo<(Vec<LayoutRect>, f32)>) -> Div {
    let mut panel = div().relative().w(PANEL_WIDTH).dyn_style(move |s| s.h(layout.with(|l| l.1)));
    for (i, view) in views.into_iter().enumerate() {
        panel = panel.child(
            div()
                .absolute()
                .dyn_style(move |s| {
                    let r = layout.with(|l| l.0[i]);
                    let s = s.left(r.x).top(r.y).w(r.width).h(r.height);
                    if r.height > 0.0 { s } else { s.hidden() }
                })
                .child(view),
        );
    }
    panel
}

#[cfg(test)]
mod tests {
    use super::*;

    use DetailWidth::{Caption, Full, Half};

    #[test]
    fn two_entries_sit_side_by_side_and_the_rest_stack() {
        let entries = [(Half, 50.0), (Full, 30.0), (Half, 50.0), (Full, 40.0)];
        let (rects, height) = panel_layout(&entries, 216.0);

        // 216 - 2 * 16 padding = 184 inner; (184 - 12 gap) / 2 = 86 half.
        assert_eq!(rects[0], LayoutRect::new(16.0, 16.0, 86.0, 50.0));
        assert_eq!(rects[2], LayoutRect::new(114.0, 16.0, 86.0, 50.0));
        assert_eq!(rects[1], LayoutRect::new(16.0, 78.0, 184.0, 30.0));
        assert_eq!(rects[3], LayoutRect::new(16.0, 120.0, 184.0, 40.0));
        assert_eq!(height, 160.0);
    }

    #[test]
    fn captions_go_on_top_and_empty_ones_take_no_space() {
        let entries = [(Half, 50.0), (Caption, 16.0), (Full, 30.0)];
        let (rects, height) = panel_layout(&entries, 216.0);
        assert_eq!(rects[1], LayoutRect::new(16.0, 16.0, 184.0, 16.0));
        assert_eq!(rects[0].y, 44.0);
        assert_eq!(height, 136.0);

        let entries = [(Half, 50.0), (Caption, 0.0), (Full, 30.0)];
        let (rects, height) = panel_layout(&entries, 216.0);
        assert_eq!(rects[0].y, 16.0);
        assert_eq!(height, 108.0);
    }

    #[test]
    fn extra_half_entries_start_a_new_row() {
        let entries = [(Half, 50.0), (Half, 60.0), (Half, 50.0)];
        let (rects, height) = panel_layout(&entries, 216.0);

        assert_eq!(rects[2], LayoutRect::new(16.0, 88.0, 86.0, 50.0));
        assert_eq!(height, 138.0);
    }

    #[test]
    fn the_row_is_as_wide_as_its_icons() {
        assert_eq!(row_width(0), ROW_PADDING * 2.0);
        assert_eq!(row_width(4), ROW_PADDING * 2.0 + 4.0 * ICON_SIZE + 3.0 * ICON_GAP);
    }
}
