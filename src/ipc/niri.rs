use std::{collections::HashMap, thread};

use calloop::channel::Sender;
use log::warn;
use niri_ipc::{socket::Socket, Request, Response, Window};

use crate::ipc::{events::IPCEvent, IpcTrait};

#[allow(unused)]
pub struct NiriIpc {
    socket: Socket,
    sender: Sender<IPCEvent>,
}

impl IpcTrait for NiriIpc {
    fn get_current_window_name(&mut self) -> Option<String> {
        let reply = self.socket.send(Request::FocusedWindow).ok()?.ok()?;
        if let Response::FocusedWindow(Some(window)) = reply {
            window.title
        } else {
            None
        }
    }

    fn get_current_workspace(&mut self) -> u32 {
        let reply = self.socket.send(Request::Workspaces).unwrap().unwrap();
        if let Response::Workspaces(workspace) = reply {
            workspace
                .iter()
                .find(|w| w.is_focused)
                .map(|w| w.id as u32)
                .unwrap_or(0)
        } else {
            0
        }
    }
}

impl NiriIpc {
    pub fn new(sender: Sender<IPCEvent>) -> Result<Self, String> {
        let socket = Socket::connect().map_err(|e| e.to_string())?;
        let event_socket = Socket::connect().map_err(|e| e.to_string())?;

        Self::listen_ipc_events(event_socket, sender.clone());

        Ok(Self { socket, sender })
    }

    fn listen_ipc_events(mut socket: Socket, sender: Sender<IPCEvent>) {
        let reply = socket.send(Request::EventStream).unwrap();
        if !matches!(reply, Ok(Response::Handled)) {
            warn!("Failed to connect event stream");
            return;
        }

        let mut focused_ws_id = 0;
        let mut focused_window_id = 0;
        let mut window_map: HashMap<u64, Window> = HashMap::new();

        thread::spawn(move || {
            let mut read_events = socket.read_events();

            loop {
                let events = match read_events() {
                    Ok(e) => e,
                    Err(e) => {
                        warn!("Disconnect from niri ipc. {e:?}");
                        break;
                    }
                };

                println!("{events:?}");
                println!("{focused_window_id}");

                match events {
                    niri_ipc::Event::WorkspacesChanged { workspaces } => {}
                    niri_ipc::Event::WorkspaceUrgencyChanged { id, urgent } => {}
                    niri_ipc::Event::WorkspaceActivated { id, .. } => {
                        if id != focused_ws_id {
                            let _ =
                                sender.send(IPCEvent::ForcusedWorkspaceChanged(focused_ws_id, id));
                            focused_ws_id = id;
                        }
                    }
                    niri_ipc::Event::WorkspaceActiveWindowChanged {
                        workspace_id,
                        active_window_id,
                    } => {}
                    niri_ipc::Event::WindowsChanged { windows } => {
                        window_map = windows.into_iter().map(|w| (w.id, w)).collect();
                    }
                    niri_ipc::Event::WindowOpenedOrChanged { window } => {
                        if window.is_focused {
                            focused_window_id = window.id;
                        }

                        if window.clone().id == focused_window_id {
                            let _ = sender
                                .send(IPCEvent::FocusedWindowTitleChanged(window.clone().title));
                        }

                        window_map.insert(window.id, window);
                    }
                    niri_ipc::Event::WindowClosed { id } => {
                        window_map.remove(&id);
                    }
                    niri_ipc::Event::WindowFocusChanged { id } => {
                        let title = id
                            .and_then(|id| window_map.get(&id))
                            .and_then(|w| w.title.clone());

                        let _ = sender.send(IPCEvent::FocusedWindowTitleChanged(title));
                    }
                    niri_ipc::Event::WindowFocusTimestampChanged {
                        id,
                        focus_timestamp,
                    } => {}
                    niri_ipc::Event::WindowUrgencyChanged { id, urgent } => {}
                    niri_ipc::Event::WindowLayoutsChanged { changes } => {}
                    niri_ipc::Event::KeyboardLayoutsChanged { keyboard_layouts } => {}
                    niri_ipc::Event::KeyboardLayoutSwitched { idx } => {}
                    niri_ipc::Event::OverviewOpenedOrClosed { is_open } => {}
                    niri_ipc::Event::ConfigLoaded { failed } => {}
                    niri_ipc::Event::ScreenshotCaptured { path } => {}
                    niri_ipc::Event::CastsChanged { casts } => {}
                    niri_ipc::Event::CastStartedOrChanged { cast } => {}
                    niri_ipc::Event::CastStopped { stream_id } => {}
                }
            }
        });
    }
}
