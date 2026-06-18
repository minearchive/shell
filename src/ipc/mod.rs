use log::warn;

use crate::ipc::{hyprland::HyprlandIpc, niri::NiriIpc};

mod hyprland;
mod niri;

#[allow(unused)]
pub(crate) trait IpcTrait {
    fn get_current_window_name(&mut self) -> Option<String>;
    fn get_current_workspace(&mut self) -> Option<u32>;
}

pub enum WindowManagerIPC {
    Niri(niri::NiriIpc),
    Hyprland(hyprland::HyprlandIpc),
    Unknown,
    ConnectionFailed(String),
    None,
}

impl Default for WindowManagerIPC {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowManagerIPC {
    pub fn new() -> Self {
        if let Ok(wm) = std::env::var("XDG_CURRENT_DESKTOP") {
            if wm == "niri" {
                match NiriIpc::new() {
                    Ok(ipc) => return Self::Niri(ipc),
                    Err(e) => {
                        warn!("Failed to connect to niri ipc: {e}");
                        return Self::ConnectionFailed(e);
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

    fn get_current_workspace(&mut self) -> Option<u32> {
        match self {
            WindowManagerIPC::Niri(ipc) => ipc.get_current_workspace(),
            WindowManagerIPC::Hyprland(ipc) => ipc.get_current_workspace(),
            _ => None,
        }
    }
}
