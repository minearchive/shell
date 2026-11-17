//! The system tray: the icons of programs that register a StatusNotifierItem, in the bar left of
//! the toolbox. A left click does what the program says a click does (usually opening its window),
//! a right click opens its menu, and the wheel goes to it as it would on any other bar.
//!
//! The row takes no room while there is no tray, so a session without one looks as it did.

use std::path::{Path, PathBuf};
use std::{rc::Rc, sync::Arc};

use anti_webui::core::images::Bitmap;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui::shell::{icon_path, MenuToggle, TrayIcon, TrayMenuItem};
use anti_webui::style::Color;
use anti_webui_md3e::{components::sized_icon, icons};
use ui_core::util::elide;

use crate::{
    state::State,
    ui::{measure, palette, BarSlot, IslandFrame, IslandPart, IslandPlacement, IslandRequest, Part, BAR_HEIGHT},
};

const ICON_SIZE: f32 = 20.0;
const ICON_GAP: f32 = 10.0;
const ROW_PADDING: f32 = 12.0;
/// Below the toolbox's panel, which the user opened on purpose, and above the OSD.
const PRIORITY: u8 = 140;
/// The menu's width, and the room its entries have.
const MENU_WIDTH: f32 = 260.0;
const MENU_PADDING: f32 = 8.0;
const MENU_ROW_HEIGHT: f32 = 34.0;
const MENU_ROW_PADDING: f32 = 12.0;
const MENU_TEXT: f32 = 14.0;
const MENU_ICON: f32 = 16.0;
const MENU_ICON_GAP: f32 = 10.0;
const MENU_RADIUS: f32 = 12.0;
const SEPARATOR_HEIGHT: f32 = 1.0;
/// Above and below a separator.
const SEPARATOR_MARGIN: f32 = 4.0;
/// A menu with more entries than this is cut off: an island cannot scroll.
const MENU_MAX_ROWS: usize = 12;
/// How far a submenu's entries are indented per level.
const INDENT: f32 = 16.0;

/// The item's picture at the size the bar draws it.
fn picture(item: &TrayItem) -> Option<Arc<Bitmap>> {
    item.icon.best(ICON_SIZE as u32 * 2)
}

/// The item as it is now: it changes under a key that stays.
fn now(key: &str) -> Option<TrayItem> {
    tray().items().into_iter().find(|i| i.key() == key)
}

/// The file for an item that names its icon instead of sending pictures, as most do: in the
/// directory the program brings, else in the icon theme.
fn themed(icon: &TrayIcon) -> Option<PathBuf> {
    let name = icon.name.as_deref()?;
    // ponytail: only the top of the program's own directory, not a theme laid out inside it.
    let own = icon.theme_path.as_deref().and_then(|dir| {
        ["png", "svg"].iter().map(|ext| Path::new(dir).join(format!("{name}.{ext}"))).find(|p| p.is_file())
    });
    own.or_else(|| icon_path(name, ICON_SIZE as u32 * 2))
}

/// An item's icon: its pictures, else its icon by name, else the first letter of its title.
fn icon_view(key: String) -> View {
    dyn_view(move || {
        let Some(item) = now(&key) else { return div().into_view() };
        if let Some(picture) = picture(&item) {
            return bitmap_view(move || Some(picture.clone())).w(ICON_SIZE).h(ICON_SIZE).into_view();
        }
        if let Some(path) = themed(&item.icon) {
            return img(path).w(ICON_SIZE).h(ICON_SIZE).into_view();
        }
        {
            // Many items leave the title empty; the id at least says whose it is.
            let letter = item.title.chars().chain(item.id.chars()).next().map(String::from).unwrap_or_default();
            div()
                .w(ICON_SIZE)
                .h(ICON_SIZE)
                .flex()
                .items_center()
                .justify_center()
                .child(text(letter).text_size(ICON_SIZE * 0.7))
                .into_view()
        }
    })
}

/// The tray's part: the row of icons in the bar, and the menu of the one that was clicked.
pub fn tray_part(state: &State) -> Part {
    // Which item's menu is open, by its key. `None` while none is.
    let menu_for = signal(None::<String>);
    // Kept while the island collapses, so the menu has something to show.
    let shown = signal(None::<TrayItem>);
    {
        effect(move || {
            let item = menu_for.get().and_then(|key| now(&key));
            // An item that goes away while its menu is open takes the menu with it.
            if item.is_none() && menu_for.get_untracked().is_some() {
                untrack(|| menu_for.set(None));
            }
            untrack(|| shown.set(item));
        });
    }

    let row = {
        let colors = state.clone();
        let open = menu_for;
        div()
            .h(BAR_HEIGHT)
            .flex()
            .flex_row()
            .items_center()
            .gap(ICON_GAP)
            .dyn_style(move |s| {
                // No tray, no room: the row takes nothing and the bar looks as it did.
                let s = s.text_color(palette(&colors, |t| t.on_surface));
                if tray().items().is_empty() { s.hidden() } else { s.px(ROW_PADDING) }
            })
            .child(for_each(
                move || tray().items(),
                |item| item.key(),
                move |item| {
                    let key = item.key();
                    let clicked = item.clone();
                    let menu_key = key.clone();
                    let scroll = item.clone();
                    div()
                        .w(ICON_SIZE)
                        .h(ICON_SIZE)
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor(CursorIcon::Pointer)
                        .child(icon_view(key))
                        .on_pointer_down(move |e| match e.button {
                            // An item that is only a menu opens that instead of activating.
                            Some(PointerButton::Primary) if clicked.item_is_menu => open_menu(&open, &clicked),
                            Some(PointerButton::Primary) => {
                                close_menu(&open, &menu_key);
                                clicked.activate(0, 0);
                            }
                            Some(PointerButton::Secondary) if clicked.has_menu => open_menu(&open, &clicked),
                            Some(PointerButton::Secondary) => {
                                close_menu(&open, &menu_key);
                                clicked.context_menu(0, 0);
                            }
                            Some(PointerButton::Middle) => {
                                close_menu(&open, &menu_key);
                                clicked.secondary_activate(0, 0);
                            }
                            _ => {}
                        })
                        .on_scroll(move |e| {
                            // A notch one way or the other, which is what the items expect.
                            // `signum` of 0.0 is 1.0, so a sideways scroll would read as one.
                            // ponytail: one notch per event, so a touchpad sends a burst; sum
                            // `delta_y` up to a threshold if that bothers an item.
                            if e.delta_y != 0.0 {
                                scroll.scroll(-e.delta_y.signum() as i32, true);
                            }
                        })
                },
            ))
    };

    let request = {
        memo(move || {
            let item = shown.get()?;
            Some(IslandRequest {
                width: MENU_WIDTH,
                height: menu_height(&menu_rows(&item)),
                placement: IslandPlacement::Joined { center_x: None },
                priority: PRIORITY,
                passive: false,
            })
        })
    };

    let content = {
        let colors = state.clone();
        div()
            .w(MENU_WIDTH)
            .p(MENU_PADDING)
            .flex()
            .flex_col()
            .dyn_style(move |s| s.text_color(palette(&colors, |t| t.on_surface)))
            .child(dyn_view(move || match shown.get() {
                Some(item) => menu_view(item, menu_for).into_view(),
                None => div().into_view(),
            }))
    };

    Part {
        bar: Some((BarSlot::Right, row.into_view())),
        overlay: None,
        island: Some(IslandPart {
            request,
            content: content.into_view(),
            close: Rc::new(move || menu_for.set(None)),
            frame: signal(None::<IslandFrame>),
        }),
    }
}

fn open_menu(open: &Signal<Option<String>>, item: &TrayItem) {
    let key = item.key();
    if open.get_untracked().as_ref() != Some(&key) {
        // Some programs fill their menu only when told it is about to be shown.
        item.menu_about_to_show(0);
    }
    open.update(|o| *o = (*o != Some(key.clone())).then_some(key));
}

/// The island's height for these rows: what `menu_view` draws, so the two cannot drift apart.
fn menu_height(rows: &[Row]) -> f32 {
    let content: f32 = if rows.is_empty() {
        // The item's title in place of a menu.
        MENU_ROW_HEIGHT
    } else {
        let shown = rows.iter().take(MENU_MAX_ROWS);
        shown.map(|r| if r.separator { SEPARATOR_HEIGHT + SEPARATOR_MARGIN * 2.0 } else { MENU_ROW_HEIGHT }).sum()
    };
    MENU_PADDING * 2.0 + content
}

fn close_menu(open: &Signal<Option<String>>, key: &str) {
    open.update(|o| {
        if o.as_deref() == Some(key) {
            *o = None;
        }
    });
}

/// A menu entry flattened for drawing: its depth, so a submenu's entries sit indented under it.
struct Row {
    depth: usize,
    label: String,
    enabled: bool,
    checked: bool,
    separator: bool,
    id: i32,
}

fn menu_rows(item: &TrayItem) -> Vec<Row> {
    flatten(&item.menu())
}

/// The entries of a menu, submenus flattened in under their parent and indented.
fn flatten(list: &[TrayMenuItem]) -> Vec<Row> {
    fn walk(list: &[TrayMenuItem], depth: usize, out: &mut Vec<Row>) {
        for entry in list {
            if !entry.visible {
                continue;
            }
            out.push(Row {
                depth,
                label: entry.label.clone(),
                enabled: entry.enabled,
                checked: matches!(entry.toggle, Some(MenuToggle::Check(true) | MenuToggle::Radio(true))),
                separator: entry.separator,
                id: entry.id,
            });
            // Submenus are shown in place, indented, rather than as a second menu: an island
            // cannot open another.
            if !entry.children.is_empty() {
                walk(&entry.children, depth + 1, out);
            }
        }
    }
    let mut rows = Vec::new();
    walk(list, 0, &mut rows);
    rows
}

fn menu_view(item: TrayItem, open: Signal<Option<String>>) -> Div {
    let rows = menu_rows(&item);
    if rows.is_empty() {
        // No menu: say what the item is instead of an empty card.
        let room = MENU_WIDTH - MENU_PADDING * 2.0 - MENU_ROW_PADDING * 2.0;
        let title = elide(&item.title, room, measure(MENU_TEXT));
        return div()
            .h(MENU_ROW_HEIGHT)
            .px(MENU_ROW_PADDING)
            .flex()
            .items_center()
            .child(text(title).text_size(MENU_TEXT));
    }
    let mut list = div().flex().flex_col();
    for row in rows.into_iter().take(MENU_MAX_ROWS) {
        if row.separator {
            list = list.child(
                div().h(SEPARATOR_HEIGHT).my(SEPARATOR_MARGIN).bg(Color::from_rgba(0x80808040)),
            );
            continue;
        }
        let room = MENU_WIDTH - MENU_PADDING * 2.0 - MENU_ROW_PADDING * 2.0 - MENU_ICON - MENU_ICON_GAP;
        let label = elide(&row.label, room, measure(MENU_TEXT));
        let (clicked, id) = (item.clone(), row.id);
        let mut entry = div()
            .h(MENU_ROW_HEIGHT)
            .rounded(MENU_RADIUS)
            .flex()
            .flex_row()
            .items_center()
            .gap(MENU_ICON_GAP)
            .pl(MENU_ROW_PADDING + row.depth as f32 * INDENT)
            .pr(MENU_ROW_PADDING)
            .dyn_style(move |s| if row.enabled { s.cursor(CursorIcon::Pointer) } else { s.opacity(0.38_f32) })
            .child(
                div().w(MENU_ICON).flex().justify_center().child(if row.checked {
                    sized_icon(icons::CHECK, MENU_ICON).into_view()
                } else {
                    div().into_view()
                }),
            )
            .child(text(label).text_size(MENU_TEXT).whitespace_nowrap());
        if row.enabled {
            entry = entry.on_click(move |_| {
                clicked.menu_clicked(id);
                open.set(None);
            });
        }
        list = list.child(entry);
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i32, label: &str) -> TrayMenuItem {
        TrayMenuItem { id, label: label.to_owned(), enabled: true, visible: true, ..Default::default() }
    }

    #[test]
    fn hidden_entries_are_left_out() {
        let mut hidden = entry(2, "Hidden");
        hidden.visible = false;
        let labels: Vec<_> = flatten(&[entry(1, "Shown"), hidden]).into_iter().map(|r| r.label).collect();
        assert_eq!(labels, ["Shown"]);
    }

    #[test]
    fn submenus_are_flattened_one_level_in() {
        let mut parent = entry(1, "Parent");
        parent.children = vec![entry(2, "Child")];
        let rows = flatten(&[parent, entry(3, "Other")]);
        let seen: Vec<_> = rows.iter().map(|r| (r.depth, r.label.as_str())).collect();
        assert_eq!(seen, [(0, "Parent"), (1, "Child"), (0, "Other")]);
    }

    #[test]
    fn a_check_or_radio_that_is_on_is_checked() {
        let mut checked = entry(1, "Checked");
        checked.toggle = Some(MenuToggle::Check(true));
        let mut unchecked = entry(2, "Unchecked");
        unchecked.toggle = Some(MenuToggle::Check(false));
        let mut radio = entry(3, "Radio");
        radio.toggle = Some(MenuToggle::Radio(true));
        let is_checked: Vec<_> = flatten(&[checked, unchecked, radio]).into_iter().map(|r| r.checked).collect();
        assert_eq!(is_checked, [true, false, true]);
    }

    #[test]
    fn the_height_counts_a_separator_as_thin_and_an_empty_menu_as_one_row() {
        let mut separator = entry(2, "");
        separator.separator = true;
        let rows = flatten(&[entry(1, "One"), separator]);
        let thin = SEPARATOR_HEIGHT + SEPARATOR_MARGIN * 2.0;
        assert_eq!(menu_height(&rows), MENU_PADDING * 2.0 + MENU_ROW_HEIGHT + thin);
        assert_eq!(menu_height(&[]), MENU_PADDING * 2.0 + MENU_ROW_HEIGHT);
    }

    #[test]
    fn separators_are_kept_as_rows() {
        let mut separator = entry(2, "");
        separator.separator = true;
        let rows = flatten(&[entry(1, "One"), separator]);
        assert_eq!(rows.len(), 2);
        assert!(rows[1].separator);
    }
}
