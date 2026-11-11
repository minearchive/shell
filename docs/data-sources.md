# Adding a data source

Use this for anything that runs in the background and reports state to the bar: D-Bus
services, sysfs, compositor IPC.

## First, check AntiWebUI

AntiWebUI has reactive providers behind `shell-*` cargo features: `shell-audio`
(PipeWire), `shell-session` (logind, brightness), `shell-battery` (UPower), `shell-network`
(NetworkManager), `shell-bluetooth` (BlueZ), `shell-power-profiles`, `shell-media` (MPRIS),
`shell-niri`, `shell-system` (CPU, memory, temperatures), and more; see its
`anti-webui/Cargo.toml`. Each is an object of signals updated from events. The volume and
brightness come from there (`anti_webui::shell::audio()` / `session()` in `State::start`).
If one covers your data, enable its feature on `anti-webui` in the root `Cargo.toml` and
copy its value into `State` with an `effect`, writing inside `untrack` so the write doesn't
subscribe the effect to what it writes:

```rust
let session = anti_webui::shell::session();
effect(move || {
    let brightness = session.brightness().map(|b| b as f32);
    untrack(|| system.update(|s| s.brightness = brightness));
});
```

## Otherwise, a provider thread

References in the tree:

- `src/dbus/warp.rs` — a small D-Bus client: polls on a timer and reacts to a signal, with a
  command channel. **Copy this** for a poll-or-signal source.
- `src/dbus/mpris.rs` — an event stream, one thread per player.
- `src/dbus/kdeconnect/` — one channel, many event variants, a task per device signal.
- `src/dbus/notification.rs` — a D-Bus *service* (`org.freedesktop.Notifications`).
- `src/system.rs` — a plain polling thread for several values at once.

The provider owns a thread and takes a `UiSender<T>` (`anti_webui::prelude::UiSender`).
Async sources build a `tokio` current-thread runtime inside that thread.

```rust
pub fn start(sender: UiSender<BatteryLevel>) {
    std::thread::spawn(move || loop {
        if let Some(level) = read_level() {
            let _ = sender.send(level);
        }
        std::thread::sleep(Duration::from_secs(5));
    });
}
```

### Wiring it into `State`

In `State::start` (`src/state.rs`), make a signal, start the provider with a `ui_channel`
whose closure runs on the UI thread and updates the signal, and add the signal to `State`:

```rust
let battery = signal(None);
my_provider::start(ui_channel(move |level: BatteryLevel| battery.set(Some(level))));
// … then `battery` in `Self { .. }`
```

Components read `state.battery` and follow it. Nothing else is needed for a repaint.

- Avoid waking readers for nothing: `update_untracked` plus `notify()` only when something
  changed, as the MPRIS position channel does.
- Keep decoding and disk work on the provider's thread. Images become an
  `anti_webui::style::ImageSource` there (`player_art::pixels`).
- If the provider takes commands, add its sender to `Commands`. Dropping a sender closes the
  provider's command channel, so keep it even before anything sends.

## Provider status

| Provider | Module | Notes |
|----------|--------|-------|
| MPRIS | `src/dbus/mpris.rs` | Event stream, one thread per player; positions polled |
| Cover art | `src/dbus/player_art.rs` | Polls `mpris:artUrl`; decodes with `image`, remote art via `curl` |
| Notifications | `src/dbus/notification.rs` | Implements `org.freedesktop.Notifications` |
| Cloudflare WARP | `src/dbus/warp.rs` | SNI signal + CLI poll |
| KDE Connect | `src/dbus/kdeconnect/` | Connect/disconnect; not shown anywhere yet |
| Volume | AntiWebUI `shell-audio` | PipeWire events |
| Brightness | AntiWebUI `shell-session` | inotify on the backlight |
| Network, Bluetooth, battery, power profile | `src/system.rs` | 5s poll |
| Compositor | `src/ipc/` | niri implemented, Hyprland a stub |
