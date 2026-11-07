use std::io::{ErrorKind, Read};

use log::{debug, info, warn};
use smithay_client_toolkit::data_device_manager::{
    data_device::DataDeviceHandler,
    data_offer::{DataOfferHandler, DragOffer},
    data_source::DataSourceHandler,
    WritePipe,
};
use wayland_client::{
    protocol::{
        wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
        wl_data_source::WlDataSource, wl_surface::WlSurface,
    },
    Connection, QueueHandle,
};

use crate::Shell;

/// Mime types we know how to log on drop, in preference order.
const PREFERRED_DROP_MIME_TYPES: &[&str] =
    &["text/uri-list", "text/plain;charset=utf-8", "text/plain"];

fn pick_drop_mime(mime_types: &[String]) -> Option<String> {
    PREFERRED_DROP_MIME_TYPES
        .iter()
        .find_map(|preferred| mime_types.iter().find(|m| m.as_str() == *preferred))
        .or_else(|| mime_types.first())
        .cloned()
}

fn finish_offer(offer: &DragOffer) {
    offer.finish();
    offer.destroy();
}

impl DataDeviceHandler for Shell {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
        x: f64,
        y: f64,
        wl_surface: &WlSurface,
    ) {
        info!("DnD enter @{x:.2},{y:.2}");

        if self.screen_for(wl_surface).is_none() {
            warn!("DnD enter on a surface that belongs to no screen, ignoring");
            return;
        }
        self.dragging = Some(wl_surface.clone());

        let Some(drag_offer) = self.drag_offer() else {
            warn!("DnD enter without a drag offer");
            return;
        };

        let mime_types = drag_offer.with_mime_types(<[String]>::to_vec);
        if let Some(mime) = pick_drop_mime(&mime_types) {
            drag_offer.accept_mime_type(drag_offer.serial, Some(mime));
        }
        drag_offer.set_actions(DndAction::Copy, DndAction::Copy);

        info!("DnD offer: {mime_types:?}");
        self.dispatch_to(wl_surface, |ui| ui.on_drag_enter(&mime_types));
    }

    fn leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _data_device: &WlDataDevice) {
        if let Some(surface) = self.dragging.take() {
            self.dispatch_to(&surface, |ui| ui.on_drag_leave());
        }
    }

    fn motion(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
        x: f64,
        y: f64,
    ) {
        if let Some(surface) = self.dragging.clone() {
            self.dispatch_to(&surface, |ui| ui.on_drag_motion(x, y));
        }
    }

    fn selection(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
    ) {
        debug!("Clipboard selection changed (unhandled: this bar is a DnD target only)");
    }

    fn drop_performed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
    ) {
        info!("DnD drop performed");

        let Some(surface) = self.dragging.take() else {
            warn!("Drop performed without a preceding DnD enter, ignoring");
            return;
        };

        let Some(offer) = self.drag_offer() else {
            warn!("Drop without a drag offer");
            return;
        };

        let mime_types = offer.with_mime_types(<[String]>::to_vec);
        let Some(mime) = pick_drop_mime(&mime_types) else {
            warn!("Drop offered no mime types");
            finish_offer(&offer);
            return;
        };

        let read_pipe = match offer.receive(mime.clone()) {
            Ok(pipe) => pipe,
            Err(err) => {
                warn!("Failed to receive drag-and-drop offer: {err}");
                finish_offer(&offer);
                return;
            }
        };

        offer.accept_mime_type(offer.serial, Some(mime.clone()));
        offer.set_actions(DndAction::Copy, DndAction::Copy);

        let mut data = Vec::new();
        let source_offer = offer.clone();
        let register = self
            .loop_handle
            .insert_source(read_pipe, move |_, f, shell: &mut Shell| {
                // SAFETY: the fd stays open (and thus valid) until this closure returns
                // PostAction::Remove, matching smithay-client-toolkit's own data_device example.
                let f: &mut std::fs::File = unsafe { f.get_mut() };
                let mut chunk = [0; 8192];
                match f.read(&mut chunk) {
                    Ok(0) => {
                        info!("Dropped {} bytes ({mime})", data.len());
                        debug!("Dropped data: {:?}", String::from_utf8_lossy(&data));
                        shell.dispatch_to(&surface, |ui| ui.on_drop(&mime, &data));
                        finish_offer(&source_offer);
                        calloop::PostAction::Remove
                    }
                    Ok(len) => {
                        data.extend_from_slice(&chunk[..len]);
                        calloop::PostAction::Continue
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => calloop::PostAction::Continue,
                    Err(e) => {
                        warn!("Error reading dropped data: {e}");
                        finish_offer(&source_offer);
                        calloop::PostAction::Remove
                    }
                }
            });

        if let Err(err) = register {
            warn!("Failed to register drop read source: {err}");
            finish_offer(&offer);
        }
    }
}

impl DataOfferHandler for Shell {
    fn source_actions(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        offer: &mut DragOffer,
        actions: DndAction,
    ) {
        debug!("Drag source advertised actions: {actions:?}");
        offer.set_actions(DndAction::Copy, DndAction::Copy);
    }

    fn selected_action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _offer: &mut DragOffer,
        actions: DndAction,
    ) {
        debug!("Compositor selected drag action: {actions:?}");
    }
}

impl DataSourceHandler for Shell {
    fn accept_mime(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
        _mime: Option<String>,
    ) {
        debug!("DataSourceHandler::accept_mime called, but this bar never creates a drag source");
    }

    fn send_request(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
        _mime: String,
        _fd: WritePipe,
    ) {
        debug!("DataSourceHandler::send_request called, but this bar never creates a drag source");
    }

    fn cancelled(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _source: &WlDataSource) {}

    fn dnd_dropped(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _source: &WlDataSource) {
    }

    fn dnd_finished(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
    ) {
    }

    fn action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
        _action: DndAction,
    ) {
    }
}
