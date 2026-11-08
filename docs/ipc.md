# IPC events

## Overview

`ipc::listen` (`src/ipc/mod.rs`) is called once in `main()` with `ipc_tx` and picks
the backend from `$XDG_CURRENT_DESKTOP`. Each backend spawns a listener thread
that translates compositor-native events into the internal `IPCEvent` enum and
sends them over a `calloop::channel`. The closure registered with `on_msg` in
`main()` calls `shell.dispatch(|ui| ui.on_ipc(&event))`, which fans the event out
to every `Screen` and requests a redraw for the screens that returned
non-`Redraw::None`.

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

### 3. Handle in UserInterface

`UserInterface::on_ipc` (`src/ui.rs`) currently just fans every event out to
every component and folds their `Redraw` results — it doesn't match on
variants itself:

```rust
pub fn on_ipc(&mut self, event: &IPCEvent) -> Redraw {
    self.fan_out(|c| c.on_ipc(event))
}
```

If `MonitorAdded` needs to update shared `UIState` (rather than being handled
entirely inside a component), do it in the IPC closure in `main()` before
dispatching, following the pattern the MPRIS and WARP closures use for their own
state fields (see [docs/components.md](components.md#7-reading-shared-state)).

### 4. Handle (or ignore) in components

Each `Component::on_ipc(&mut self, events: &IPCEvent) -> Redraw` receives
every variant (default body: `Redraw::None`). Match on the ones you care
about; add a catch-all `_ => Redraw::None` arm to ignore the rest. Return
`Redraw::Now` when the event changes what the component draws.

## Backend status

| Backend | File | Status |
|---------|------|--------|
| niri | `src/ipc/niri.rs` | Implemented — workspace + window focus events |
| hyprland | `src/ipc/hyprland.rs` | Stub — empty handlers; `start_listener` blocks its caller |
