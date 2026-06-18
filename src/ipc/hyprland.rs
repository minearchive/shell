use crate::ipc::IpcTrait;

pub struct HyprlandIpc {}

impl IpcTrait for HyprlandIpc {
    fn get_current_window_name(&mut self) -> Option<String> {
        todo!()
    }

    fn get_current_workspace(&mut self) -> Option<u32> {
        todo!()
    }
}

impl HyprlandIpc {
    pub fn new() -> Self {
        Self {}
    }
}
