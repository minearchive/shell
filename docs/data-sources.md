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

## Sources that are not signals

Some state is not a value the bar reads but something it holds: an inhibitor, a child
process, a file. `src/toggles.rs` has the pattern:

- **A held resource** — logind's idle inhibitor is an `OwnedFd`. `State::start` keeps it in a
  `RefCell` inside an effect and replaces it with `None` when the signal goes off, which
  drops (releases) the descriptor. The resource's lifetime is the signal's.
- **A child process** — the night light spawns `gammastep` with `PR_SET_PDEATHSIG` so it dies
  with the bar, and `toggles::stop` sends `SIGTERM` (which makes gammastep restore the
  screen's colours) and reaps it on a thread, never blocking the UI thread.
- **A remembered flag** — a file that exists while a toggle is on, under
  `$XDG_STATE_HOME/my_shell/`. See
  [docs/config.md](config.md#remembered-state).

Start and stop these from effects in `State::start`, not from the views that draw the
toggles: a view is rebuilt as an island opens and closes, so it must not own a resource.

## Derived state

Not everything needs a provider. `State::warn_on_low_battery` (`src/state.rs`) turns
`system` into a notification with an effect: it watches `Battery::low()` and posts into the
same `NotificationStore` the D-Bus server fills, keyed by a fixed id (`LOW_BATTERY_ID`) so it
can take the notification back when the battery charges. Prefer this for anything that is a
pure function of signals already in `State`.


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
| Mic mute | AntiWebUI `shell-audio` | the default source, read straight from the provider (not in `State`) |
| Idle inhibitor | `src/toggles.rs` | logind `Inhibit`, held as an fd while keep awake is on |
| Night light | `src/toggles.rs` | a `gammastep` child process while night light is on |
| Network, Bluetooth, battery, power profile | `src/system.rs` | 5s poll |
| Low battery warning | `src/state.rs` | derived from `system` into `notifications` |
| Compositor | `src/ipc/` | niri implemented, Hyprland a stub |
