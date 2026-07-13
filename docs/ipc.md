# IPC events

## Overview

`WindowManagerIPC` (`src/ipc/mod.rs`) is an enum dispatcher instantiated once in
`main.rs:160` from `$XDG_CURRENT_DESKTOP`. It spawns a listener thread that
translates compositor-native events into the internal `IPCEvent` enum and sends
them over a `calloop::channel`. The main loop fans each event out to every screen
(`main.rs:161`), which calls `UserInterface::on_ipc`.

## Adding a new IPCEvent variant

### 1. Extend the enum

`src/ipc/events.rs`:
```rust
#[derive(Clone)]
pub enum IPCEvent {
    FocusedWorkspaceChanged(u64, u64),
    FocusedWindowTitleChanged(Option<String>),
    MonitorAdded(String),   // new
}
```

`IPCEvent` **must** `#[derive(Clone)]` — the main loop fans it to every screen.

### 2. Emit from a backend

`src/ipc/niri.rs` (or `hyprland.rs`):
```rust
let _ = sender.send(IPCEvent::MonitorAdded(name));
```

### 3. Handle in UserInterface

`src/ui.rs` → `UserInterface::on_ipc` (around line 96):
```rust
IPCEvent::MonitorAdded(name) => {
    // update UIState if needed
    let _ = self.sender.send(UiEvent::RequestRedraw(self.idx));
}
```

Only send `RequestRedraw` if the bar actually needs to repaint.

### 4. Handle (or ignore) in components

Each `Component::on_ipc` receives every variant. Match on the ones you care
about; add a catch-all `_ => {}` arm to ignore the rest.

## Backend status

| Backend | File | Status |
|---------|------|--------|
| niri | `src/ipc/niri.rs` | Implemented — workspace + window focus events |
| hyprland | `src/ipc/hyprland.rs` | Stub — all `IpcTrait` methods are `todo!()` |

When `XDG_CURRENT_DESKTOP=hyprland` the enum delegates to the Hyprland backend
automatically, so implementing the stub methods there is all that is needed.
