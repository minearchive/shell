//! A surface along the top edge of each screen for the notification on display
//! ([`Banner`]). It always spans the screen's width at a fixed height, transparent and
//! click-through except for the card itself. It sits on the overlay layer, above the
//! catcher, so the card keeps its clicks while the toolbox is open.

use skia_safe::{surfaces, ImageInfo};
use smithay_client_toolkit::{
    compositor::Region,
    seat::pointer::PointerEvent,
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerSurface},
        WaylandSurface,
    },
};
use wayland_client::protocol::{wl_output::WlOutput, wl_shm, wl_surface::WlSurface};

use crate::{
    components::notifications::{Banner, EASING_ID, SURFACE_HEIGHT},
    ui::Redraw,
    Shell,
};

pub struct BannerSurface {
    layer: LayerSurface,
    /// From the last configure; zero until the first.
    size: (u32, u32),
    needs_redraw: bool,
    frame_pending: bool,
    /// Whether the last frame painted nothing, so that another blank one can be skipped.
    blank: bool,
    /// What the input region was last set from: the card's rect, if it was up.
    input: Option<[i32; 4]>,
    banner: Banner,
}

impl Shell {
    pub(crate) fn create_banner(&self, output: &WlOutput) -> BannerSurface {
        let surface = self.compositor_state.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(
            &self.qh,
            surface,
            Layer::Overlay,
            Some("ApplicationShell-banner"),
            Some(output),
        );
        layer.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, SURFACE_HEIGHT);
        // Below any panel along the top, without pushing windows down itself.
        layer.set_exclusive_zone(0);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();

        let easing = self
            .animation_config
            .read()
            .unwrap()
            .easings()
            .get(EASING_ID)
            .cloned();
        BannerSurface {
            layer,
            size: (0, 0),
            needs_redraw: false,
            frame_pending: false,
            blank: false,
            input: Some([0; 4]),
            banner: Banner::new(self.ui_tx.clone(), easing),
        }
    }

    fn banner_for(&self, surface: &WlSurface) -> Option<usize> {
        self.screen.iter().position(|s| {
            s.banner
                .as_ref()
                .is_some_and(|b| b.layer.wl_surface() == surface)
        })
    }

    /// Handles a configure if it is for a banner, and returns whether it was.
    pub(crate) fn configure_banner(&mut self, layer: &LayerSurface, size: (u32, u32)) -> bool {
        let Some(idx) = self.banner_for(layer.wl_surface()) else {
            return false;
        };
        if let Some(banner) = &mut self.screen[idx].banner {
            banner.size = (size.0, SURFACE_HEIGHT);
            banner.blank = false;
        }
        self.draw_banner(idx);
        true
    }

    /// Handles a frame callback if it is for a banner, and returns whether it was.
    pub(crate) fn banner_frame(&mut self, surface: &WlSurface) -> bool {
        let Some(idx) = self.banner_for(surface) else {
            return false;
        };
        let Some(banner) = &mut self.screen[idx].banner else {
            return true;
        };
        banner.frame_pending = false;
        if banner.needs_redraw {
            self.draw_banner(idx);
        }
        true
    }

    /// Handles a pointer event if it is on a banner, and returns whether it was.
    pub(crate) fn banner_pointer(&mut self, event: &PointerEvent) -> bool {
        let Some(idx) = self.banner_for(&event.surface) else {
            return false;
        };
        let redraw = self.screen[idx]
            .banner
            .as_mut()
            .map_or(Redraw::None, |b| b.banner.on_cursor(event));
        if redraw != Redraw::None {
            self.request_banner_redraw(idx);
        }
        true
    }

    /// Handles the compositor closing a banner, and returns whether it was one. The
    /// screen carries on without it.
    pub(crate) fn close_banner(&mut self, layer: &LayerSurface) -> bool {
        let Some(idx) = self.banner_for(layer.wl_surface()) else {
            return false;
        };
        self.screen[idx].banner = None;
        true
    }

    pub(crate) fn set_banner_easing(&mut self, easing: &ui_core::animation::parser::Easing) {
        for banner in self.screen.iter_mut().filter_map(|s| s.banner.as_mut()) {
            banner.banner.set_easing(easing.clone());
        }
    }

    pub(crate) fn request_banner_redraw(&mut self, idx: usize) {
        let Some(banner) = self.screen.get_mut(idx).and_then(|s| s.banner.as_mut()) else {
            return;
        };
        banner.needs_redraw = true;
        if !banner.frame_pending {
            self.draw_banner(idx);
        }
    }

    fn draw_banner(&mut self, idx: usize) {
        let theme = self.theme.read().unwrap();
        let Some(banner) = self.screen.get_mut(idx).and_then(|s| s.banner.as_mut()) else {
            return;
        };
        let (width, height) = (banner.size.0 as i32, banner.size.1 as i32);
        banner.needs_redraw = false;
        if width == 0 || height == 0 || (banner.blank && banner.banner.is_blank(&self.state)) {
            return;
        }

        let stride = width * 4;
        let Ok((buffer, pixels)) =
            self.pool
                .create_buffer(width, height, stride, wl_shm::Format::Argb8888)
        else {
            return;
        };
        let info = ImageInfo::new_n32_premul((width, height), None);
        let Some(mut skia_surface) = surfaces::wrap_pixels(&info, pixels, stride as usize, None)
        else {
            return;
        };
        let redraw = banner.banner.draw(
            skia_surface.canvas(),
            &self.state,
            &self.font,
            theme.theme(),
            width as f32,
        );
        banner.blank = banner.banner.is_blank(&self.state);

        let rect = banner.banner.rect().map(|r| {
            [
                r.x.floor() as i32,
                r.y.floor() as i32,
                r.width.ceil() as i32,
                r.height.ceil() as i32,
            ]
        });
        let surface = banner.layer.wl_surface();
        if banner.input != rect {
            if let Ok(region) = Region::new(&self.compositor_state) {
                if let Some([x, y, w, h]) = rect {
                    region.add(x, y, w, h);
                }
                surface.set_input_region(Some(region.wl_region()));
                banner.input = rect;
            }
        }

        if redraw == Redraw::Animating {
            banner.needs_redraw = true;
            banner.frame_pending = true;
            surface.frame(&self.qh, surface.clone());
        }
        surface.damage_buffer(0, 0, width, height);
        if buffer.attach_to(surface).is_err() {
            return;
        }
        surface.commit();
    }
}
