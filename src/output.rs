use std::{num::NonZeroU32, sync::Arc};

use smithay_client_toolkit::{
    compositor::CompositorHandler,
    output::{OutputHandler, OutputState},
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
};
use wayland_client::{
    protocol::{
        wl_output::{Transform, WlOutput},
        wl_surface::WlSurface,
    },
    Connection, QueueHandle,
};

use crate::{
    ui::{UserInterface, BAR_HEIGHT, ISLAND_MAX_HEIGHT},
    Screen, Shell,
};

impl CompositorHandler for Shell {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surfaces: &WlSurface, _: u32) {
        if let Some(idx) = self.screen_for(surfaces) {
            self.screen[idx].frame_pending = false;
            if self.screen[idx].needs_redraw {
                self.draw(idx);
            }
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &WlOutput,
    ) {
    }
}

impl OutputHandler for Shell {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
        let surface = self.compositor_state.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Top,
            Some("ApplicationShell"),
            Some(&output),
        );

        layer.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, BAR_HEIGHT + ISLAND_MAX_HEIGHT);
        layer.set_exclusive_zone(BAR_HEIGHT as i32);
        // The bar must never steal keyboard focus from the user's window.
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();

        self.screen.push(Screen {
            layer,
            width: 0,
            height: 0,
            first_configure: true,
            needs_redraw: false,
            frame_pending: false,
            input_region: [0; 5],
            ui: UserInterface::new(
                Arc::clone(&self.theme),
                &self.animation_config,
                self.ui_tx.clone(),
                self.system_commands.clone(),
            ),
            output,
            catcher: None,
        });
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: WlOutput) {
        self.screen.retain(|s| s.output != output);
    }
}

impl LayerShellHandler for Shell {
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surfaces: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        if self.configure_catcher(surfaces, configure.new_size) {
            return;
        }

        let Some(idx) = self.screen.iter().position(|s| &s.layer == surfaces) else {
            return;
        };

        self.screen[idx].width = NonZeroU32::new(configure.new_size.0).map_or(256, NonZeroU32::get);
        self.screen[idx].height =
            NonZeroU32::new(configure.new_size.1).map_or(256, NonZeroU32::get);

        if self.screen[idx].first_configure {
            self.screen[idx].first_configure = false;
            self.draw(idx);
        }
    }

    /// Only this surface is gone (typically its monitor was unplugged); the
    /// other screens keep running, and `new_output` covers a replugged monitor.
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &LayerSurface) {
        if !self.close_catcher(surface) {
            self.screen.retain(|s| &s.layer != surface);
        }
    }
}
