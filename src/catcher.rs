//! A transparent surface over the whole screen, up only while something wants to know
//! about presses outside the bar: Wayland shows a client nothing of other windows'
//! input, so the only way to see such a press is to be the one it lands on.

use smithay_client_toolkit::{
    compositor::Region,
    seat::pointer::{PointerEvent, PointerEventKind},
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerSurface},
        WaylandSurface,
    },
};
use wayland_client::protocol::wl_shm;

use ui_core::geometry::LayoutRect;

use crate::{
    ui::{Redraw, BAR_HEIGHT, ISLAND_MAX_HEIGHT},
    Shell,
};

pub struct Catcher {
    layer: LayerSurface,
    /// From the last configure; zero until the first.
    size: (i32, i32),
    /// What the input region was last set from.
    holes: Vec<[i32; 4]>,
}

/// What the catcher must leave uncovered on a screen of `size`: the bar and the island,
/// whose `island` rect is in the coordinates of the bar's surface. The catcher may be
/// stacked over the bar, and these need to keep receiving their own input.
fn catcher_holes(size: (i32, i32), island: Option<LayoutRect>) -> Vec<[i32; 4]> {
    let (width, height) = size;
    let bar_height = BAR_HEIGHT as i32;
    let surface_top = height - bar_height - ISLAND_MAX_HEIGHT as i32;

    let mut holes = vec![[0, height - bar_height, width, bar_height]];
    if let Some(r) = island {
        holes.push([
            r.x.floor() as i32,
            surface_top + r.y.floor() as i32,
            r.width.ceil() as i32,
            r.height.ceil() as i32,
        ]);
    }
    holes
}

impl Shell {
    /// Puts the catcher up or takes it down to match what screen `idx` wants.
    pub(crate) fn sync_catcher(&mut self, idx: usize) {
        if !self.screen[idx].ui.captures_outside_press() {
            self.screen[idx].catcher = None;
            return;
        }
        if self.screen[idx].catcher.is_none() {
            let surface = self.compositor_state.create_surface(&self.qh);
            let layer = self.layer_shell.create_layer_surface(
                &self.qh,
                surface,
                Layer::Top,
                Some("ApplicationShell-catcher"),
                Some(&self.screen[idx].output),
            );
            layer.set_anchor(Anchor::all());
            // Covers the bar's exclusive zone too, so the bar's position is predictable.
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.commit();
            self.screen[idx].catcher = Some(Catcher {
                layer,
                size: (0, 0),
                holes: Vec::new(),
            });
        }
        self.update_catcher_region(idx);
    }

    fn update_catcher_region(&mut self, idx: usize) {
        let island = self.screen[idx].ui.island_rect();
        let Some(catcher) = &mut self.screen[idx].catcher else {
            return;
        };
        let (width, height) = catcher.size;
        if width == 0 || height == 0 {
            return;
        }
        let holes = catcher_holes(catcher.size, island);
        if catcher.holes == holes {
            return;
        }
        let Ok(region) = Region::new(&self.compositor_state) else {
            return;
        };
        region.add(0, 0, width, height);
        for [x, y, w, h] in &holes {
            region.subtract(*x, *y, *w, *h);
        }
        let surface = catcher.layer.wl_surface();
        surface.set_input_region(Some(region.wl_region()));
        surface.commit();
        catcher.holes = holes;
    }

    /// Handles a configure if it is for a catcher, and returns whether it was.
    pub(crate) fn configure_catcher(&mut self, surface: &LayerSurface, size: (u32, u32)) -> bool {
        let Some(idx) = self
            .screen
            .iter()
            .position(|s| s.catcher.as_ref().is_some_and(|c| &c.layer == surface))
        else {
            return false;
        };
        let (width, height) = (size.0 as i32, size.1 as i32);
        if width == 0 || height == 0 {
            return true;
        }
        // ponytail: a full-screen buffer only for its transparency, and the pool never
        // shrinks back. A 1x1 buffer with wp_viewport would avoid both.
        let Ok((buffer, pixels)) =
            self.pool
                .create_buffer(width, height, width * 4, wl_shm::Format::Argb8888)
        else {
            return true;
        };
        pixels.fill(0);

        let Some(catcher) = &mut self.screen[idx].catcher else {
            return true;
        };
        catcher.size = (width, height);
        catcher.holes.clear();
        let wl_surface = catcher.layer.wl_surface();
        if buffer.attach_to(wl_surface).is_err() {
            return true;
        }
        wl_surface.damage_buffer(0, 0, width, height);
        wl_surface.commit();
        self.update_catcher_region(idx);
        true
    }

    /// Takes down the catcher if `surface` is one, and returns whether it was.
    pub(crate) fn close_catcher(&mut self, surface: &LayerSurface) -> bool {
        for screen in &mut self.screen {
            if screen.catcher.as_ref().is_some_and(|c| &c.layer == surface) {
                screen.catcher = None;
                return true;
            }
        }
        false
    }

    /// Handles a pointer event if it landed on a catcher, and returns whether it did.
    pub(crate) fn catcher_pointer(&mut self, event: &PointerEvent) -> bool {
        let Some(idx) = self.screen.iter().position(|s| {
            s.catcher
                .as_ref()
                .is_some_and(|c| c.layer.wl_surface() == &event.surface)
        }) else {
            return false;
        };
        if matches!(event.kind, PointerEventKind::Press { .. })
            && self.screen[idx].ui.on_outside_press() != Redraw::None
        {
            self.request_redraw(idx);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes_cover_the_bar_and_map_the_island_onto_the_screen() {
        let size = (1920, 1080);
        let bar = [0, 1080 - BAR_HEIGHT as i32, 1920, BAR_HEIGHT as i32];
        assert_eq!(catcher_holes(size, None), [bar]);

        // The bar's surface starts this far down the screen.
        let surface_top = 1080 - BAR_HEIGHT as i32 - ISLAND_MAX_HEIGHT as i32;
        let island = LayoutRect::new(1540.0, 44.5, 380.0, 255.5);
        assert_eq!(
            catcher_holes(size, Some(island)),
            [bar, [1540, surface_top + 44, 380, 256]]
        );
    }
}
