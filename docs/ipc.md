# IPC events

## Overview

`ipc::listen` (`src/ipc/mod.rs`) is called once from `State::start` with a `ui_channel`
sender and picks the backend from `$XDG_CURRENT_DESKTOP`. Each backend spawns a listener
thread that translates compositor-native events into the internal `IPCEvent` enum. The
channel's closure in `State::start` matches on the event and sets the matching signal
(`focused_workspace`, `focused_title`).

AntiWebUI also has `shell-niri` / `shell-hyprland` / `shell-sway` features with workspaces
as signals. Consider them before extending the hand-written backends.

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

### 2. Emit from a backend

`src/ipc/niri.rs` (or `hyprland.rs`):
```rust
let _ = sender.send(IPCEvent::MonitorAdded(name));
```

### 3. Put it in `State`

Add a signal to `State` (`src/state.rs`) and set it in the `ipc::listen` closure's `match`.
Components read the signal; see [docs/components.md](components.md#4-shared-state).

## Backend status

| Backend | File | Status |
|---------|------|--------|
| niri | `src/ipc/niri.rs` | Implemented — workspace + window focus events |
| hyprland | `src/ipc/hyprland.rs` | Stub — empty handlers; `start_listener` blocks its caller |
