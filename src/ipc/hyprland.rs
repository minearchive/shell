use std::fmt;

use calloop::channel::Sender;
use hyprland::{
    data::{Client, Clients},
    default_instance,
    event_listener::EventListener,
    instance::Instance,
    shared::HyprData,
};

use crate::ipc::{events::IPCEvent, IpcTrait};

pub enum HyprlandIPCError {
    EventListenerError(hyprland::error::HyprError),
    NoInstance,
}

impl fmt::Display for HyprlandIPCError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HyprlandIPCError::EventListenerError(e) => write!(f, "event listener error: {e}"),
            HyprlandIPCError::NoInstance => write!(f, "no hyprland instance found"),
        }
    }
}

pub struct HyprlandIpc {
    _event_listener: EventListener,
    instance: &'static Instance,
}

impl IpcTrait for HyprlandIpc {
    fn get_current_window_name(&mut self) -> Option<String> {
        todo!()
    }

    fn get_current_workspace(&mut self) -> u32 {
        todo!()
    }
}

impl HyprlandIpc {
    pub fn new(sender: Sender<IPCEvent>) -> Result<Self, HyprlandIPCError> {
        let mut event_listener = EventListener::new();

        let _sender = sender.clone();

        event_listener.add_workspace_changed_handler(move |_event| {});
        event_listener.add_workspace_added_handler(|_event| {});
        event_listener.add_workspace_deleted_handler(|_event| {});
        event_listener.add_workspace_moved_handler(|_event| {});
        event_listener.add_workspace_renamed_handler(|_event| {});
        event_listener.add_active_monitor_changed_handler(|_event| {});
        event_listener.add_active_window_changed_handler(|_event| {});
        event_listener.add_fullscreen_state_changed_handler(|_event| {});
        event_listener.add_monitor_added_handler(|_event| {});
        event_listener.add_monitor_removed_handler(|_event| {});
        event_listener.add_window_opened_handler(|_event| {});
        event_listener.add_window_closed_handler(|_event| {});
        event_listener.add_window_moved_handler(|_event| {});
        event_listener.add_special_removed_handler(|_event| {});
        event_listener.add_changed_special_handler(|_event| {});
        event_listener.add_layout_changed_handler(|_event| {});
        event_listener.add_sub_map_changed_handler(|_event| {});
        event_listener.add_layer_opened_handler(|_event| {});
        event_listener.add_layer_closed_handler(|_event| {});
        event_listener.add_float_state_changed_handler(|_event| {});
        event_listener.add_urgent_state_changed_handler(|_event| {});
        event_listener.add_window_title_changed_handler(|_event| {});
        event_listener.add_screencast_handler(|_event| {});
        event_listener.add_config_reloaded_handler(|| {});
        event_listener.add_ignore_group_lock_state_changed_handler(|_event| {});
        event_listener.add_lock_groups_state_changed_handler(|_event| {});
        event_listener.add_window_pinned_handler(|_event| {});
        event_listener.add_group_toggled_handler(|_event| {});
        event_listener.add_window_moved_into_group_handler(|_event| {});
        event_listener.add_window_moved_out_of_group_handler(|_event| {});
        event_listener.add_unknown_handler(|_event| {});

        event_listener
            .start_listener()
            .map_err(|e| HyprlandIPCError::EventListenerError(e))?;

        let instance = default_instance().map_err(|e| HyprlandIPCError::NoInstance)?;

        Ok(Self {
            _event_listener: event_listener,
            instance: instance,
        })
    }
}
