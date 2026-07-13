# Adding a data source

Use this pattern when adding providers for audio, battery, network, backlight,
D-Bus services, or anything else that runs in the background and pushes state
updates to the bar.

Two reference styles already exist in the tree:

- `src/dbus/mpris.rs` — event-stream (D-Bus) client. Spawns one thread per
  player and sends `(PlayerState, Event)` tuples. More complex than you need
  for a single-value source.
- `src/dbus/warp.rs`, `src/dbus/kdeconnect.rs`, `src/dbus/notifications.rs` —
  minimal single-channel clients that send one message type. **Copy one of
  these** as the template for a new provider.

The pattern has three moving parts:

1. An event enum (`Clone` — the main loop fans it out to every screen).
2. A client with `init(sender)` that owns a background thread.
3. The wiring in `main.rs`, plus fan-out in `UserInterface` and the `Component`
   trait.

## Step-by-step (battery example)

### 1. Define the event type

New file `src/sys/battery.rs`:

```rust
use std::sync::OnceLock;
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

`Clone` is required: `main.rs` sends the same event to every screen.

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

Use `std::sync::OnceLock` if the client must be a singleton (see `MprisClient`).
For push-based sources (D-Bus), block on the stream instead of polling — see
`src/dbus/warp.rs` for the minimal shape.

### 3. Wire the channel in main.rs

Insert this next to the other `insert_source` blocks (the MPRIS block starts at
`main.rs:147`). The closure fans the message out to every screen and calls the
matching `UserInterface::on_*`:

```rust
let (battery_tx, battery_channel) = channel::channel::<BatteryEvent>();
BatteryClient::init(battery_tx);
loop_handle
    .insert_source(battery_channel, |event, _, shell| {
        if let calloop::channel::Event::Msg(ev) = event {
            for screen in &mut shell.screen {
                screen.ui.on_battery(&ev);
            }
        }
    })
    .unwrap();
```

### 4. Add on_battery to UserInterface

`src/ui.rs` — fan out to every component and (optionally) cache the latest value
in `UIState` so `draw` can read it. Mirror `UserInterface::on_warp`:

```rust
pub fn on_battery(&mut self, event: &BatteryEvent) {
    self.components.iter_mut().for_each(|c| c.on_battery(event));
    // optional: self.state.battery = Some(event.clone());
}
```

### 5. Add on_battery to the Component trait

`src/ui.rs` — give it a default no-op so existing components still compile:

```rust
fn on_battery(&mut self, _event: &BatteryEvent) {}
```

Only `draw` is required; every other trait method has a default body. The full
trait currently exposes: `draw`, `on_cursor`, `on_ipc`, `on_mpris`,
`on_kde_connect_event`, `on_warp`, `on_notification`, `on_easing_updated`.

### 6. Surface state via UIState (optional)

If a component needs the latest battery value inside `draw`, store it in
`UIState` (step 4) and read `state.battery` there. Add the field in `src/ui.rs`,
initialise it in `UIState::new`, and update it inside `on_battery`. Send
`UiEvent::RequestRedraw(self.idx)` from `on_battery` only when a repaint is
actually needed — `on_warp` updates `UIState` without forcing a redraw, so the
handler that changes something visible is responsible for triggering the repaint.

## Provider status

| Provider | Module | Status | Notes |
|----------|--------|--------|-------|
| MPRIS | `src/dbus/mpris.rs` | implemented | Event stream, one thread per player |
| Cloudflare WARP | `src/dbus/warp.rs` | implemented | Single-channel D-Bus client |
| KDE Connect | `src/dbus/kdeconnect.rs` | implemented | D-Bus org.kde.kdeconnect |
| Notifications | `src/dbus/notifications.rs` | implemented | org.freedesktop.Notifications |
| Audio / volume | `src/dbus/audio.rs` | planned | PipeWire / PulseAudio via D-Bus or pipewire-rs |
| Battery | `src/sys/battery.rs` | planned | Poll `/sys/class/power_supply/` |
| Backlight / light level | `src/sys/backlight.rs` | planned | Poll `/sys/class/backlight/<name>/brightness`; ambient-light sensors live under `/sys/bus/iio/devices/.../in_illuminance_raw` |
| Network | `src/dbus/network.rs` | planned | NetworkManager D-Bus API |
| Monitor mirroring | `src/ipc/niri.rs` | planned | Extend niri IPC backend |
