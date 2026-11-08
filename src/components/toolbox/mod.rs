//! The bar's status icons, and the panel of details they open.

mod items;

use m3_widget::{icon::BOX_SIZE, Icon};
use skia_safe::{Canvas, Rect};
use smithay_client_toolkit::seat::pointer::{
    PointerEvent as SctkPointerEvent, PointerEventKind as SctkKind,
};
use tokio::sync::mpsc::UnboundedSender;

use ui_core::{
    animation::parser::Easing, font::FontBook, geometry::LayoutRect, pointer::button,
    scheme::ColorTheme,
};

use self::items::{default_items, DetailWidth, ToolboxItem};
use crate::{
    dbus::warp::{WarpCommand, WarpStatus},
    system::{SystemCommand, SystemStatus},
    ui::{
        to_core_event, BarSlot, Component, IslandFrame, IslandPlacement, IslandRequest, Redraw,
        UIState, BAR_HEIGHT,
    },
};

const ICON_GAP: f32 = 14.0;
const ROW_PADDING: f32 = 20.0;

const PANEL_WIDTH: f32 = 380.0;
const PANEL_PADDING: f32 = 16.0;
const PANEL_GAP: f32 = 12.0;
/// Above a normal notification, so one can't pull the panel away from under a drag.
const PANEL_PRIORITY: u8 = 150;

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

/// Where an icon `progress` of the way from `from` to `to` is.
pub fn lerp_box(from: Rect, to: Rect, progress: f32) -> Rect {
    Rect::from_xywh(
        from.left + (to.left - from.left) * progress,
        from.top + (to.top - from.top) * progress,
        from.width(),
        from.height(),
    )
}

pub struct Toolbox {
    items: Vec<Box<dyn ToolboxItem>>,
    rects: Vec<LayoutRect>,
    panel_height: f32,
    open: bool,
    /// The row's width on the last draw, for hit testing.
    row_width: f32,
}

impl Toolbox {
    pub fn new(
        commands: UnboundedSender<SystemCommand>,
        warp_commands: UnboundedSender<WarpCommand>,
    ) -> Self {
        let mut toolbox = Self {
            items: default_items(commands, warp_commands),
            rects: Vec::new(),
            panel_height: 0.0,
            open: false,
            row_width: 0.0,
        };
        toolbox.lay_out(&SystemStatus::default());
        toolbox
    }

    /// Entries can come and go with the status, such as a battery's caption.
    fn lay_out(&mut self, status: &SystemStatus) {
        let entries: Vec<_> = self
            .items
            .iter()
            .map(|item| (item.width(), item.height(status)))
            .collect();
        (self.rects, self.panel_height) = panel_layout(&entries, PANEL_WIDTH);
    }
}

impl Toolbox {
    fn close(&mut self) -> Redraw {
        if self.open {
            self.open = false;
            Redraw::Now
        } else {
            Redraw::None
        }
    }

    /// The row toggles the panel, and anywhere else in the bar dismisses it. `x` and `y`
    /// are relative to the row, so the rest of the bar is out of its range.
    fn on_bar_press(&mut self, x: f64, y: f64) -> Redraw {
        let in_row =
            (0.0..self.row_width as f64).contains(&x) && (0.0..BAR_HEIGHT as f64).contains(&y);
        if in_row {
            self.open = !self.open;
            Redraw::Now
        } else {
            self.close()
        }
    }
}

impl Component for Toolbox {
    fn bar_slot(&self) -> BarSlot {
        BarSlot::Right
    }

    fn bar_width(&self, state: &UIState, _fonts: &FontBook) -> f32 {
        let icons = self
            .items
            .iter()
            .filter(|item| item.bar_icon(&state.system).is_some())
            .count() as f32;
        ROW_PADDING * 2.0 + icons * BOX_SIZE + (icons - 1.0).max(0.0) * ICON_GAP
    }

    /// The bar's icons live here rather than in `draw`, so that they can fly over the
    /// panel to their entries as it opens.
    fn draw_overlay(
        &mut self,
        canvas: &Canvas,
        island: Option<IslandFrame>,
        state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> Redraw {
        self.row_width = self.bar_width(state, fonts);
        let top = (BAR_HEIGHT as f32 - BOX_SIZE) / 2.0;
        let progress = island.map_or(0.0, |frame| frame.progress);

        let mut x = ROW_PADDING;
        for (item, rect) in self.items.iter().zip(&self.rects) {
            let Some(icon) = item.bar_icon(&state.system) else {
                continue;
            };
            let in_bar = Rect::from_xywh(x, top, BOX_SIZE, BOX_SIZE);
            let at = island.map_or(in_bar, |frame| {
                let in_panel = item
                    .icon_box(*rect)
                    .with_offset((frame.content.x, frame.content.y));
                lerp_box(in_bar, in_panel, progress)
            });
            let alpha = if item.icon_fades() {
                1.0 - progress
            } else {
                1.0
            };
            icon.draw(canvas, at, theme.on_surface.with_alpha(alpha));
            x += BOX_SIZE + ICON_GAP;
        }

        // The row is empty once its icons have left: this is what closes the panel.
        let close = Rect::from_xywh((self.row_width - BOX_SIZE) / 2.0, top, BOX_SIZE, BOX_SIZE);
        Icon::Close.draw(canvas, close, theme.on_surface.with_alpha(progress));
        Redraw::None
    }

    fn on_cursor(&mut self, event: &SctkPointerEvent) -> Redraw {
        match event.kind {
            SctkKind::Press { button, .. } if button == button::LEFT => {
                self.on_bar_press(event.position.0, event.position.1)
            }
            _ => Redraw::None,
        }
    }

    fn on_easing_updated(&mut self, id: &str, easing: &Easing) -> Redraw {
        for item in &mut self.items {
            item.on_easing_updated(id, easing);
        }
        Redraw::None
    }

    fn on_warp(&mut self, _status: &WarpStatus) -> Redraw {
        if self.open {
            Redraw::Now
        } else {
            Redraw::None
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
        _bar_width: f32,
    ) -> Option<IslandRequest> {
        self.lay_out(&state.system);
        self.open.then(|| IslandRequest {
            width: PANEL_WIDTH,
            height: self.panel_height,
            placement: IslandPlacement::Corner {
                from_width: self.bar_width(state, fonts),
            },
            priority: PANEL_PRIORITY,
        })
    }

    fn draw_island(
        &mut self,
        canvas: &Canvas,
        _size: (f32, f32),
        state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> Redraw {
        let mut animating = false;
        for (item, rect) in self.items.iter_mut().zip(&self.rects) {
            animating |= item.draw(canvas, *rect, state, fonts, theme);
        }
        if animating {
            Redraw::Animating
        } else {
            Redraw::None
        }
    }

    fn on_island_cursor(&mut self, event: &SctkPointerEvent) -> Redraw {
        let Some(event) = to_core_event(event) else {
            return Redraw::None;
        };
        let mut dirty = false;
        for item in &mut self.items {
            dirty |= item.on_pointer(&event);
        }
        if dirty {
            Redraw::Now
        } else {
            Redraw::None
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;
    use crate::system::{Battery, BatteryState};

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
    fn battery_caption_appears_with_a_battery() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut toolbox = Toolbox::new(tx, mpsc::unbounded_channel().0);

        toolbox.lay_out(&SystemStatus::default());
        let without = toolbox.panel_height;

        toolbox.lay_out(&SystemStatus {
            battery: Some(Battery {
                percent: 72,
                state: BatteryState::Discharging,
            }),
            ..Default::default()
        });
        assert_eq!(toolbox.panel_height, without + 16.0 + PANEL_GAP);
    }

    #[test]
    fn extra_half_entries_start_a_new_row() {
        let entries = [(Half, 50.0), (Half, 60.0), (Half, 50.0)];
        let (rects, height) = panel_layout(&entries, 216.0);

        assert_eq!(rects[2], LayoutRect::new(16.0, 88.0, 86.0, 50.0));
        assert_eq!(height, 138.0);
    }

    #[test]
    fn icons_travel_from_the_bar_to_their_panel_entries() {
        let from = Rect::from_xywh(100.0, 18.0, 24.0, 24.0);
        let to = Rect::from_xywh(-200.0, -240.0, 24.0, 24.0);

        assert_eq!(lerp_box(from, to, 0.0), from);
        assert_eq!(lerp_box(from, to, 1.0), to);
        assert_eq!(
            lerp_box(from, to, 0.5),
            Rect::from_xywh(-50.0, -111.0, 24.0, 24.0)
        );
    }

    #[test]
    fn warp_status_repaints_only_an_open_panel() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut toolbox = Toolbox::new(tx, mpsc::unbounded_channel().0);
        let status = WarpStatus::default();

        assert_eq!(toolbox.on_warp(&status), Redraw::None);
        toolbox.open = true;
        assert_eq!(toolbox.on_warp(&status), Redraw::Now);
    }

    #[test]
    fn row_toggles_and_presses_elsewhere_only_close() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut toolbox = Toolbox::new(tx, mpsc::unbounded_channel().0);
        toolbox.row_width = 100.0;

        assert_eq!(
            toolbox.on_bar_press(-500.0, 30.0),
            Redraw::None,
            "stays closed"
        );
        assert!(!toolbox.captures_outside_press());

        assert_eq!(toolbox.on_bar_press(50.0, 30.0), Redraw::Now);
        assert!(toolbox.open, "the row opens");
        assert!(toolbox.captures_outside_press());

        assert_eq!(toolbox.on_bar_press(50.0, 30.0), Redraw::Now);
        assert!(!toolbox.open, "the row closes");

        toolbox.open = true;
        assert_eq!(toolbox.on_bar_press(-500.0, 30.0), Redraw::Now);
        assert!(!toolbox.open, "the rest of the bar closes");

        toolbox.open = true;
        assert_eq!(toolbox.on_outside_press(), Redraw::Now);
        assert!(!toolbox.open, "outside the bar closes");
    }

    #[test]
    fn bar_shows_the_four_icons_and_the_panel_fits_the_headroom() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let toolbox = Toolbox::new(tx, mpsc::unbounded_channel().0);

        let status = SystemStatus {
            battery: Some(Battery {
                percent: 72,
                state: BatteryState::Discharging,
            }),
            ..Default::default()
        };
        let mut toolbox = toolbox;
        toolbox.lay_out(&status);
        let icons = toolbox.items.iter().filter_map(|i| i.bar_icon(&status));
        assert_eq!(icons.count(), 4);
        assert!(toolbox.panel_height <= crate::ui::ISLAND_MAX_HEIGHT as f32);
    }
}
