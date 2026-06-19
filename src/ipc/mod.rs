use std::u32;

use calloop::channel::Sender;
use log::warn;

use crate::ipc::{events::IPCEvent, hyprland::HyprlandIpc, niri::NiriIpc};

pub mod events;
mod hyprland;
mod niri;

#[allow(unused)]
pub(crate) trait IpcTrait {
    fn get_current_window_name(&mut self) -> Option<String>;
    fn get_current_workspace(&mut self) -> u32;
}

pub enum WindowManagerIPC {
    Niri(niri::NiriIpc),
    Hyprland(hyprland::HyprlandIpc),
    Unknown,
    ConnectionFailed(),
    None,
}

impl WindowManagerIPC {
    pub fn new(sender: Sender<IPCEvent>) -> Self {
        if let Ok(wm) = std::env::var("XDG_CURRENT_DESKTOP") {
            if wm == "niri" {
                match NiriIpc::new(sender) {
                    Ok(ipc) => return Self::Niri(ipc),
                    Err(e) => {
                        warn!("Failed to connect to niri ipc: {e}");
                        return Self::ConnectionFailed();
                    }
                }
            }

            if wm == "hyprland" {
                return Self::Hyprland(HyprlandIpc::new());
            }

            Self::Unknown
        } else {
            Self::None
        }
    }
}

impl IpcTrait for WindowManagerIPC {
    fn get_current_window_name(&mut self) -> Option<String> {
        match self {
            WindowManagerIPC::Niri(ipc) => ipc.get_current_window_name(),
            WindowManagerIPC::Hyprland(ipc) => ipc.get_current_window_name(),
            _ => None,
        }
    }

    fn get_current_workspace(&mut self) -> u32 {
        match self {
            WindowManagerIPC::Niri(ipc) => ipc.get_current_workspace(),
            WindowManagerIPC::Hyprland(ipc) => ipc.get_current_workspace(),
            _ => u32::MAX,
        }
    }
}
