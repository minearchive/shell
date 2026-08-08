# Adding a data source

Use this pattern when adding providers for audio, battery, network, backlight,
D-Bus services, or anything else that runs in the background and pushes state
updates to the bar. This is about **bar-side** providers (`src/dbus/`,
`src/ipc/`) feeding `Component`s — unrelated to the `m3_widget` stack, see
[docs/widgets.md](widgets.md) for that.

Reference styles already exist in the tree:

- `src/dbus/mpris.rs` — event-stream (D-Bus) client. Spawns one thread per
  player and sends `(PlayerState, mpris::Event)` tuples. More complex than you
  need for a single-value source.
- `src/dbus/warp.rs` — minimal single-channel D-Bus client (`WarpStatus`),
  polling on a timer plus reacting to a StatusNotifierItem signal. **Copy this**
  as the template for a new poll-or-signal source.
- `src/dbus/kdeconnect/` (a module directory: `mod.rs` + `proxy.rs`) — one
  channel, many event variants (`KDEConnectEvent`), one task per D-Bus signal
  stream per device.
- `src/dbus/notification.rs` — implements a D-Bus *service* (not just a
  client): `org.freedesktop.Notifications`.

The pattern has three moving parts:

1. An event enum (`Clone` — the main loop fans it out to every screen).
2. A client with `init(sender)` that owns a background thread. Sync sources
   (mpris, niri) just block in the thread; async/D-Bus sources (warp,
   kdeconnect) build a `tokio::runtime::Builder::new_current_thread()` inside
   that thread and `block_on` an async `run`.
3. The wiring in `main()`, plus fan-out in `UserInterface` and the `Component`
   trait.

## Step-by-step (battery example)

### 1. Define the event type

New file `src/sys/battery.rs`:

```rust
use std::thread;
use std::time::Duration;
use calloop::channel::Sender;

#[derive(Clone)]
pub enum BatteryEvent {
    Percentage(u8),        // 0..100
    Status(BatteryStatus), // charging / discharging / full
}

#[derive(Clone)]
pub enum BatteryStatus {
    Charging,
    Discharging,
    Full,
    Unknown,
}
```

`Clone` is required: the main loop sends the same event to every screen's
`UserInterface`.

### 2. Write the client

Battery state is not pushed by the kernel, so poll `/sys/class/power_supply/`:

```rust
pub struct BatteryClient;

impl BatteryClient {
    pub fn init(sender: Sender<BatteryEvent>) {
        thread::spawn(move || {
            let base = "/sys/class/power_supply/BAT0";
            loop {
                if let Ok(s) = std::fs::read_to_string(format!("{base}/capacity")) {
                    if let Ok(pct) = s.trim().parse::<u8>() {
                        let _ = sender.send(BatteryEvent::Percentage(pct));
                    }
                }
                if let Ok(s) = std::fs::read_to_string(format!("{base}/status")) {
                    let status = match s.trim() {
                        "Charging" => BatteryStatus::Charging,
                        "Discharging" => BatteryStatus::Discharging,
                        "Full" => BatteryStatus::Full,
                        _ => BatteryStatus::Unknown,
                    };
                    let _ = sender.send(BatteryEvent::Status(status));
                }
                thread::sleep(Duration::from_secs(5));
            }
        });
    }
}
```

For push-based sources (D-Bus signals), block on the stream instead of
polling — see `src/dbus/warp.rs` for the `tokio::select!`-over-{command,
signal, timer} shape, or `src/dbus/kdeconnect/mod.rs`'s `add_device` for one
task per signal stream.

### 3. Wire the channel in `main()`

Insert this next to the other five `insert_source` blocks in `main()`. Each
existing block follows the same shape: fan the event to every screen, collect
which screens' `Component`s reported non-`Redraw::None`, and call
`shell.request_redraw` only for those — mirror it exactly:

```rust
let (battery_tx, battery_channel) = channel::channel::<BatteryEvent>();
BatteryClient::init(battery_tx);
loop_handle
    .insert_source(battery_channel, |event, _, shell| {
        if let calloop::channel::Event::Msg(ev) = event {
            let mut dirty = Vec::new();
            for (i, screen) in shell.screen.iter_mut().enumerate() {
                if screen.ui.on_battery(&ev) != Redraw::None {
                    dirty.push(i);
                }
            }
            for i in dirty {
                shell.request_redraw(i);
            }
        }
    })
    .unwrap();
```

### 4. Add `on_battery` to `UserInterface`

`src/ui.rs` — fan out to every component, fold their `Redraw` results, and
(optionally) cache the latest value in `UIState` so `draw` can read it. Mirror
`UserInterface::on_warp` exactly:

```rust
pub fn on_battery(&mut self, event: &BatteryEvent) -> Redraw {
    let redraw = self
        .components
        .iter_mut()
        .map(|c| c.on_battery(event))
        .fold(Redraw::None, Redraw::max);
    // optional: self.state.battery = Some(event.clone());
    redraw
}
```

### 5. Add `on_battery` to the `Component` trait

`src/ui.rs` — give it a default `Redraw::None` body so existing components
still compile:

```rust
fn on_battery(&mut self, _event: &BatteryEvent) -> Redraw {
    Redraw::None
}
```

Only `draw` is required; every other trait method has a default `Redraw::None`
body. The full trait today: `draw`, `on_cursor`, `on_ipc`, `on_mpris`,
`on_kde_connect_event`, `on_warp`, `on_notification`, `on_easing_updated`.
**Never leave `todo!()` in a default body** — these are called unconditionally
as events arrive and will crash the process.

### 6. Surface state via `UIState` (optional)

If a component needs the latest battery value inside `draw`, store it in
`UIState` (step 4) and read `state.battery` there. Add the field in `src/ui.rs`,
initialise it in `UIState::new`, and update it inside `on_battery`. The
`Redraw` returned from `on_battery` (step 4) is what actually triggers the
repaint — `on_warp` folds and returns its components' `Redraw` the same way,
so simply caching a value in `UIState` without a component reacting to it in
`on_warp`/`on_battery` produces no visible update until the next unrelated
redraw. See [docs/config.md](config.md) for the full `UIState` reference.

## Provider status

| Provider | Module | Status | Notes |
|----------|--------|--------|-------|
| MPRIS | `src/dbus/mpris.rs` | implemented | Event stream, one thread per player |
| Cloudflare WARP | `src/dbus/warp.rs` | implemented | Single-channel D-Bus client, hybrid SNI-signal + CLI-poll |
| KDE Connect | `src/dbus/kdeconnect/` | implemented | D-Bus `org.kde.kdeconnect`, one task per device signal |
| Notifications | `src/dbus/notification.rs` | implemented | Implements `org.freedesktop.Notifications` as a service |
| Audio / volume | `src/dbus/audio.rs` | planned | PipeWire / PulseAudio via D-Bus or pipewire-rs |
| Battery | `src/sys/battery.rs` | planned | Poll `/sys/class/power_supply/` |
| Backlight / light level | `src/sys/backlight.rs` | planned | Poll `/sys/class/backlight/<name>/brightness`; ambient-light sensors live under `/sys/bus/iio/devices/.../in_illuminance_raw` |
| Network | `src/dbus/network.rs` | planned | NetworkManager D-Bus API |
| Monitor mirroring | `src/ipc/niri.rs` | planned | Extend niri IPC backend |
