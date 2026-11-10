use anti_webui::prelude::UiSender as Sender;
use log::warn;

use crate::ipc::{events::IPCEvent, hyprland::HyprlandIpc};

pub mod events;
mod hyprland;
mod niri;

/// Forward the running compositor's events to `sender`, if we know how to.
pub fn listen(sender: Sender<IPCEvent>) {
    match std::env::var("XDG_CURRENT_DESKTOP").as_deref() {
        Ok("niri") => {
            if let Err(e) = niri::listen(sender) {
                warn!("Failed to connect to niri ipc: {e}");
            }
        }
        Ok("hyprland") => {
            if let Err(e) = HyprlandIpc::new(sender) {
                warn!("Failed to connect to hyprland ipc: {e}");
            }
        }
        _ => {}
    }
}
