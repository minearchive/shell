use crate::ipc::IpcTrait;

pub struct HyprlandIpc {}

impl IpcTrait for HyprlandIpc {
    fn get_current_window_name(&self) -> Option<String> {
        todo!()
    }

    fn get_current_workspace(&self) -> Option<u32> {
        todo!()
    }
}
