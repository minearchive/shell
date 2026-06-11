mod hyprland;
mod niri;

pub trait IpcTrait {
    fn get_current_window_name(&self) -> Option<String>;
    fn get_current_workspace(&self) -> Option<u32>;
}

pub enum WindowManager {
    Niri(niri::NiriIpc),
    Hyprland(hyprland::HyprlandIpc),
    Unknown,
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowManager {
    pub fn new() -> Self {
        Self::Unknown
    }
}
