mod hyprland;
mod niri;

pub trait IpcTrait {
    fn get_current_window_name(&self) -> Option<String>;
    fn get_current_workspace(&self) -> Option<u32>;
}

pub enum WindowManagerIPC {
    Niri(niri::NiriIpc),
    Hyprland(hyprland::HyprlandIpc),
    Unknown,
}

impl Default for WindowManagerIPC {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowManagerIPC {
    pub fn new() -> Self {
        Self::Unknown
    }
}

impl IpcTrait for WindowManagerIPC {
    fn get_current_window_name(&self) -> Option<String> {
        match self {
            WindowManagerIPC::Niri(ipc) => ipc.get_current_window_name(),
            WindowManagerIPC::Hyprland(ipc) => ipc.get_current_window_name(),
            WindowManagerIPC::Unknown => None,
        }
    }

    fn get_current_workspace(&self) -> Option<u32> {
        match self {
            WindowManagerIPC::Niri(ipc) => ipc.get_current_workspace(),
            WindowManagerIPC::Hyprland(ipc) => ipc.get_current_workspace(),
            WindowManagerIPC::Unknown => None,
        }
    }
}
