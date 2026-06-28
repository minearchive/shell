# AGENTS.md

See [CLAUDE.md](./CLAUDE.md) for build/run commands and high-level architecture.
This file adds the detail an agent needs to make changes correctly.

## Commit messages

Follow [Conventional Commits](https://www.conventionalcommits.org/): write each
message as `<type>(<optional scope>): <description>`, lowercase description, no
trailing period. Common types: `feat`, `fix`, `refactor`, `docs`, `test`,
`chore`. Use scopes that match the module being touched, e.g.
`feat(ipc): map niri workspace events`, `fix(ui): reset redraw flag`,
`refactor(font): cache typeface`.

---

## Key files at a glance

| Path | Purpose |
|------|---------|
| `src/main.rs` | Event loop wiring; `Shell` struct; `Shell::draw`; all SCTK delegates |
| `src/ui.rs` | `Component` trait; `UserInterface`; `UIState`; `UiEvent` enum |
| `src/ipc/mod.rs` | `WindowManagerIPC` dispatcher; `IpcTrait` |
| `src/ipc/events.rs` | `IPCEvent` enum (internal IPC event type) |
| `src/ipc/niri.rs` | Niri backend (fleshed out) |
| `src/ipc/hyprland.rs` | Hyprland backend (stub — `todo!()` everywhere) |
| `src/dbus/mpris.rs` | `MprisClient` background thread; `PlayerState` |
| `src/components/clock.rs` | Only live `Component` impl — good reference |
| `src/config/config.rs` | `Configuration` + hot-reload via `notify` |
| `src/font/mod.rs` | `FontBook` (key → `Typeface` cache); `sized(key, px)` |

---

## How to add a new Component

Components are the primary extension point. Each lives in `src/components/`.

### 1. Implement the trait

```rust
// src/components/my_widget.rs
use crate::ui::{Component, UIState, UiEvent};
use crate::ipc::events::IPCEvent;
use crate::dbus::mpris::PlayerState;
use crate::font::FontBook;
use mpris::Event as MprisEvent;
use skia_safe::Canvas;
use smithay_client_toolkit::seat::pointer::PointerEvent;

pub struct MyWidget { /* ... */ }

impl Component for MyWidget {
    fn draw(&self, canvas: &Canvas, state: &UIState, fonts: &FontBook) { /* ... */ }
    fn on_cursor(&self, _: &PointerEvent) {}
    fn on_ipc(&mut self, _: &IPCEvent) {}
    fn on_mpris(&mut self, _: &PlayerState, _: &MprisEvent) {}
}
```

All four methods must be implemented — use empty bodies for ones you don't need.
**Do not leave `todo!()` in `on_mpris`**: `UserInterface::on_mpris` fans out to
every component unconditionally, so a panic there crashes the whole app.

### 2. Register it

Add the module in `src/components/mod.rs`:
```rust
pub mod my_widget;
```

Instantiate it in `UserInterface::new` (`src/ui.rs:72`):
```rust
let components: Vec<Box<dyn Component>> = vec![
    Box::new(Clock::new(rx.clone(), idx, 1000, Arc::clone(&config))),
    Box::new(MyWidget::new(/* ... */)),
];
```

### 3. Triggering redraws from a background thread

A component that needs periodic updates spawns its own thread and drives redraws
via the calloop `Sender<UiEvent>` — exactly as `Clock` does:

```rust
// Store the sender; call from the background thread:
let _ = sender.send(UiEvent::RequestRedraw(screen_idx));
```

`UiEvent::RequestRedraw(idx)` is handled in `main.rs:141` by calling
`Shell::draw(idx)` directly — there is no separate "dirty" flag to set.

### 4. Using fonts

Only the fonts registered in `main.rs:184` are available by default (`"noto_sans"`
→ Noto Sans CJK JP). To use another font, send `UiEvent::RegisterFont` from a
component or provider:

```rust
sender.send(UiEvent::RegisterFont(
    "my_font".into(),
    "Roboto".into(),
    skia_safe::FontStyle::normal(),
))?;
```

Then call `fonts.sized("my_font", 24.)` in `draw`.

---

## How to add a new IPCEvent variant

1. Add the variant to the enum in `src/ipc/events.rs`.
2. Emit it from the relevant backend (`src/ipc/niri.rs` or
   `src/ipc/hyprland.rs`) via the `Sender<IPCEvent>`.
3. Handle it in `UserInterface::on_ipc` (`src/ui.rs:89`) — update `UIState`
   and send `UiEvent::RequestRedraw(self.idx)` if the bar should repaint.
4. Handle or ignore it in each `Component::on_ipc` impl.

`IPCEvent` must `#[derive(Clone)]` because the main loop fans it out to every
screen (see `main.rs:128`).

---

## How to add a new data source (audio, battery, network, …)

Follow the pattern used by `MprisClient` (`src/dbus/mpris.rs`) and the IPC
backends:

1. **Define an event type** in a new file, e.g. `src/dbus/audio.rs`:
   ```rust
   #[derive(Clone)]
   pub enum AudioEvent { VolumeChanged(f64), MuteToggled(bool) }
   ```

2. **Spawn a background thread** that polls or listens and sends events through
   a `calloop::channel::Sender<AudioEvent>`.

3. **Wire up the channel** in `main.rs` before the event loop starts:
   ```rust
   let (audio_tx, audio_channel) = channel::channel::<AudioEvent>();
   AudioClient::init(audio_tx);
   loop_handle.insert_source(audio_channel, |event, _, shell| {
       if let calloop::channel::Event::Msg(ev) = event {
           for screen in &mut shell.screen {
               screen.ui.on_audio(&ev);
           }
       }
   }).unwrap();
   ```

4. **Add `on_audio` to `UserInterface`** in `src/ui.rs` (mirror `on_mpris`):
   fan out to components, update `UIState`, send `UiEvent::RequestRedraw`.

5. **Add `on_audio` to the `Component` trait** if components need to react;
   provide a default no-op so existing components don't break:
   ```rust
   fn on_audio(&mut self, _event: &AudioEvent) {}
   ```

---

## UIState — shared render state

`UIState` (`src/ui.rs:38`) is passed by reference to every `Component::draw`
call. Fields added here become immediately available to all components:

```rust
pub struct UIState {
    pub players: HashMap<String, PlayerState>,
    pub workspace_id: String,
    pub window_title: String,
    pub padding: f32,
    // add new fields here
}
```

Initialise new fields in `UIState::new` and update them in the relevant
`UserInterface::on_*` handler.

---

## Configuration (`src/config/config.rs`)

Config is a TOML file hot-reloaded via `notify`. Path is currently hardcoded in
`main.rs:164`. `Configuration::load_and_watch` returns an `Arc<RwLock<Configuration>>`
passed to every component that needs it. Read it with `config.read().unwrap()`;
never hold the lock across `await` or across another lock acquisition.

`ColorTheme` exposes Material You colour roles (`primary`, `surface_container`,
etc.). Use `cfg.theme()` which returns either `&cfg.dark` or `&cfg.light`
depending on `cfg.is_dark`.

---

## Known sharp edges

- **`shoud_redraw` typo**: CLAUDE.md mentions this field but it does not exist
  in the current source. `Shell::draw` renders unconditionally whenever called.
  Do not introduce this field without updating the draw path to match.

- **Hyprland IPC**: all `IpcTrait` methods in `src/ipc/hyprland.rs` are
  `todo!()`. Do not call them until they are implemented; the enum arms in
  `WindowManagerIPC` delegate to them automatically via `XDG_CURRENT_DESKTOP`.

- **`on_mpris` must not panic**: see the Component section above. Every
  component's `on_mpris` is called for every MPRIS event from every player.

- **Config path is hardcoded**: `main.rs:164` points to an absolute path under
  `/home/minearchive/`. Move this to an argument or env var before shipping.

- **Fonts must be registered before use**: calling `fonts.sized` with an
  unregistered key will return a fallback font silently — no panic, but wrong
  rendering. Register in `main.rs` or via `UiEvent::RegisterFont`.

- **Build environment**: `skia-bindings` requires `LIBCLANG_PATH`,
  `LD_LIBRARY_PATH`, and EGL variables from `devenv.nix`. `cargo build` outside
  the devenv shell will fail. Always build inside the devenv (direnv handles
  this in-repo automatically).
