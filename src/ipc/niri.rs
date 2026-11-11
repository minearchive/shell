use std::{collections::HashMap, thread};

use anti_webui::prelude::UiSender as Sender;
use log::warn;
use niri_ipc::{socket::Socket, Event, Request, Response, Window};

use crate::ipc::events::IPCEvent;

pub fn listen(sender: Sender<IPCEvent>) -> Result<(), String> {
    let mut socket = Socket::connect().map_err(|e| e.to_string())?;

    let reply = socket
        .send(Request::EventStream)
        .map_err(|e| e.to_string())?;
    if !matches!(reply, Ok(Response::Handled)) {
        return Err("niri refused the event stream".to_string());
    }

    let mut focused_ws_id = 0;
    let mut window_map: HashMap<u64, Window> = HashMap::new();

    thread::spawn(move || {
        let mut read_events = socket.read_events();

        loop {
            let event = match read_events() {
                Ok(e) => e,
                Err(e) => {
                    warn!("Disconnect from niri ipc. {e:?}");
                    break;
                }
            };

            // The stream opens with `WorkspacesChanged` and `WindowsChanged` carrying the
            // current state, so the bar isn't blank until the first change.
            match event {
                Event::WorkspacesChanged { workspaces } => {
                    if let Some(ws) = workspaces.iter().find(|w| w.is_focused) {
                        if ws.id != focused_ws_id {
                            let _ = sender
                                .send(IPCEvent::FocusedWorkspaceChanged(focused_ws_id, ws.id));
                            focused_ws_id = ws.id;
                        }
                    }
                }
                Event::WorkspaceActivated { id, focused: true } if id != focused_ws_id => {
                    let _ = sender.send(IPCEvent::FocusedWorkspaceChanged(focused_ws_id, id));
                    focused_ws_id = id;
                }
                Event::WindowsChanged { windows } => {
                    let title = windows
                        .iter()
                        .find(|w| w.is_focused)
                        .and_then(|w| w.title.clone());
                    let _ = sender.send(IPCEvent::FocusedWindowTitleChanged(title));
                    window_map = windows.into_iter().map(|w| (w.id, w)).collect();
                }
                Event::WindowOpenedOrChanged { window } => {
                    if window.is_focused {
                        let _ =
                            sender.send(IPCEvent::FocusedWindowTitleChanged(window.title.clone()));
                    }
                    window_map.insert(window.id, window);
                }
                Event::WindowClosed { id } => {
                    window_map.remove(&id);
                }
                Event::WindowFocusChanged { id } => {
                    let title = id
                        .and_then(|id| window_map.get(&id))
                        .and_then(|w| w.title.clone());
                    let _ = sender.send(IPCEvent::FocusedWindowTitleChanged(title));
                }
                _ => {}
            }
        }
    });

    Ok(())
}
