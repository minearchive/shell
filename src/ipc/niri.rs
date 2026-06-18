use niri_ipc::{socket::Socket, Request, Response};

use crate::ipc::IpcTrait;

#[allow(unused)]
pub struct NiriIpc {
    socket: Socket,
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

    fn get_current_workspace(&mut self) -> Option<u32> {
        todo!()
    }
}

impl NiriIpc {
    pub fn new() -> Result<Self, String> {
        Socket::connect()
            .map(|s| Self { socket: s })
            .map_err(|e| e.to_string())
    }
}
