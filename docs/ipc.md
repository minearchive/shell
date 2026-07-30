# IPC events

## Overview

`WindowManagerIPC` (`src/ipc/mod.rs`) is an enum dispatcher instantiated once in
`main()` (`WindowManagerIPC::new(ipc_tx)`) from `$XDG_CURRENT_DESKTOP`. Each
backend's constructor spawns a listener thread that translates compositor-native
events into the internal `IPCEvent` enum and sends them over a
`calloop::channel`. The `IPCEvent` `insert_source` closure in `main()` fans
each event out to every `Screen`, calling `screen.ui.on_ipc(event.clone())`,
folds the `Redraw` results, and calls `shell.request_redraw(i)` for the
screens that returned non-`Redraw::None`.

`WindowManagerIPC` also directly implements `IpcTrait` itself (delegating to
whichever backend variant it holds), for the two synchronous getters
(`get_current_window_name`, `get_current_workspace`) — separate from the async
event stream described above.

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

`UserInterface::on_ipc` (`src/ui.rs`) currently just fans every event out to
every component and folds their `Redraw` results — it doesn't match on
variants itself:

```rust
pub fn on_ipc(&mut self, event: IPCEvent) -> Redraw {
    self.components
        .iter_mut()
        .map(|c| c.on_ipc(&event))
        .fold(Redraw::None, Redraw::max)
}
```

If `MonitorAdded` needs to update shared `UIState` (rather than being handled
entirely inside a component), add that here — matching on the new variant and
writing to `self.state` — following the pattern `on_mpris`/`on_warp` use for
their own state fields (see [docs/config.md](config.md)).

### 4. Handle (or ignore) in components

Each `Component::on_ipc(&mut self, events: &IPCEvent) -> Redraw` receives
every variant (default body: `Redraw::None`). Match on the ones you care
about; add a catch-all `_ => Redraw::None` arm to ignore the rest. Return
`Redraw::Now` when the event changes what the component draws.

## Backend status

| Backend | File | Status |
|---------|------|--------|
| niri | `src/ipc/niri.rs` | Implemented — workspace + window focus events |
| hyprland | `src/ipc/hyprland.rs` | Stub — all `IpcTrait` methods are `todo!()` |

When `XDG_CURRENT_DESKTOP=hyprland` the enum delegates to the Hyprland backend
automatically, so implementing the stub methods there is all that is needed.
