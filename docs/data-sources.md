# Adding a data source

Use this pattern when adding providers for audio, battery, network, D-Bus
services, or anything else that runs in the background and pushes state updates
to the bar. `src/dbus/mpris.rs` is the canonical reference.

## Step-by-step

### 1. Define an event type

Create a new file, e.g. `src/dbus/audio.rs`:

```rust
#[derive(Clone)]
pub enum AudioEvent {
    VolumeChanged(f64),
    MuteToggled(bool),
}
```

The event type must be `Clone` — the main loop may fan it out to multiple screens.

### 2. Write the client

Implement a struct with an `init(sender: Sender<AudioEvent>)` method that spawns
a background thread:

```rust
pub struct AudioClient;

impl AudioClient {
    pub fn init(sender: Sender<AudioEvent>) {
        thread::spawn(move || {
            loop {
                // poll or block on a D-Bus/socket stream
                let _ = sender.send(AudioEvent::VolumeChanged(vol));
            }
        });
    }
}
```

Use `std::sync::OnceLock` if the client must be a singleton (see `MprisClient`).

### 3. Wire the channel in main.rs

Before the event loop starts:

```rust
let (audio_tx, audio_channel) = channel::channel::<AudioEvent>();
AudioClient::init(audio_tx);
loop_handle
    .insert_source(audio_channel, |event, _, shell| {
        if let calloop::channel::Event::Msg(ev) = event {
            for screen in &mut shell.screen {
                screen.ui.on_audio(&ev);
            }
        }
    })
    .unwrap();
```

### 4. Add on_audio to UserInterface

`src/ui.rs`:

```rust
pub fn on_audio(&mut self, event: &AudioEvent) {
    self.components.iter_mut().for_each(|c| c.on_audio(event));
    // update UIState fields as needed
    let _ = self.sender.send(UiEvent::RequestRedraw(self.idx));
}
```

### 5. Add on_audio to the Component trait

`src/ui.rs` — give it a default no-op so existing components compile unchanged:

```rust
pub trait Component {
    fn draw(&self, canvas: &Canvas, state: &UIState, fonts: &FontBook);
    fn on_cursor(&self, events: &PointerEvent);
    fn on_ipc(&mut self, events: &IPCEvent);
    fn on_mpris(&mut self, state: &PlayerState, event: &MprisEvent);
    fn on_audio(&mut self, _event: &AudioEvent) {}  // new, default no-op
}
```

### 6. Expose state via UIState (optional)

If components need to read the latest audio state in `draw`, store it in
`UIState` and update it inside `on_audio`. See [docs/config.md](config.md) for
the UIState pattern.

## Planned providers (from TODO.md)

| Provider | Suggested module | Notes |
|----------|-----------------|-------|
| Audio | `src/dbus/audio.rs` | PipeWire / PulseAudio via D-Bus or pipewire-rs |
| Battery | `src/sys/battery.rs` | Poll `/sys/class/power_supply/` |
| Network | `src/dbus/network.rs` | NetworkManager D-Bus API |
| KDE Connect | `src/dbus/kdeconnect.rs` | D-Bus org.kde.kdeconnect |
| Notifications | `src/dbus/notifications.rs` | org.freedesktop.Notifications |
| Cloudflare WARP | `src/dbus/warp.rs` | warp-svc D-Bus API |
| Monitor mirroring | `src/ipc/niri.rs` | Extend niri IPC backend |
