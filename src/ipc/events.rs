#[derive(Clone)]
pub enum IPCEvent {
    ForcusedWorkspaceChanged(u64, u64),        //(old, new)
    FocusedWindowTitleChanged(Option<String>), // title of the newly focused window
}
