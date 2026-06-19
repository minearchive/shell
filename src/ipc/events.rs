pub enum IPCEvent {
    ForcusedWorkspaceChanged(u64, u64),    //(old, new)
    FocusedWindowChanged(Option<String>),  // title of the newly focused window
}
