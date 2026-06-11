use crate::ipc::IpcTrait;

pub struct NiriIpc {}
impl IpcTrait for NiriIpc {
    fn get_current_window_name(&self) -> Option<String> {
        todo!()
    }

    fn get_current_workspace(&self) -> Option<u32> {
        todo!()
    }
}
