# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Wayland desktop bar/shell (`wlr-layer-shell`) rendered with Skia. Despite the
crate name (`gtk_learn`), the directory name (`gtk_shell`), and GTK packages in
the Nix files, the running code uses **no GTK** — it talks raw Wayland through
`smithay-client-toolkit` and draws with `skia-safe` into SHM buffers. Treat the
GTK dependencies in `flake.nix` as stale scaffolding, not the architecture.

## Build & run

The development environment is `devShells.default` in **flake.nix**, pinned by
`flake.lock`. Enable Nix's `nix-command` and `flakes` experimental features, then
enter it with `nix develop`. With direnv installed and hooked into your shell,
run `direnv allow` once; `.envrc` uses `use flake` to load the same environment.
No devenv installation is needed.

The shell provides Rust/Cargo, Clippy, rustfmt, rust-analyzer and native build
dependencies, plus `LIBCLANG_PATH`, `LD_LIBRARY_PATH` and the EGL vendor path
needed by Skia and GL/Wayland. The EGL vendor path assumes NixOS's
`/run/opengl-driver`; running the graphical binaries requires a Wayland session.

```sh
nix develop
cargo build
RUST_LOG=debug cargo run      # env_logger reads RUST_LOG; use info/debug
cargo test
cargo test test_run -- --ignored --nocapture   # mpris event stream (needs a live MPRIS player, runs until it quits)
```

For a non-interactive build (also used by CI):

```sh
nix develop --command cargo build --locked
```

`flake.nix` also exposes a `default` package via `rustPlatform.buildRustPackage`;
day-to-day development uses the devShell and Cargo, not `nix build`.

## Architecture

This is a **Cargo workspace** of five crates plus the bar binary. The binary
(`my_shell`, root `Cargo.toml`, `src/`) is the only thing that talks Wayland;
the others are a layered, presentation-only widget toolkit:

```
ui_core   → no internal deps (skia-safe, serde)      geometry, pointer/keyboard
            events, animation, color scheme, FontBook
ui_widget → ui_core                                   Widget trait, Row/Column,
                                                        ScrollableWidget
m3_widget → ui_core, ui_widget                         Material 3 widgets
m3_test   → ui_core, m3_widget (dev harness, winit)    `cargo run -p m3_test`
macros    → nothing                                    boxed!(a, b, c) -> Vec<Box<..>>
my_shell  → ui_core, m3_widget, macros (+ calloop, wayland, zbus, tokio, …)  the bar
```

The bar's own UI is built from `Component`s. It uses `m3_widget` for icons and the
toolbox's sliders. The M3 widget stack is developed and demoed standalone via
`cargo run -p m3_test` (a winit+softbuffer gallery), not inside the bar. See
[docs/widgets.md](docs/widgets.md) for the full crate/trait reference.

**Two different, easily-confused traits exist:**
- `Component` (`src/ui.rs`, bar-only) — what the bar's UI is built from today.
- `Widget` (`ui_widget::Widget`) — what `m3_widget` implements. The bar uses one
  directly only for the toolbox's sliders.
They are not related and do not share a supertrait. See
[docs/widgets.md](docs/widgets.md) for the distinction in detail.

The bar itself is one event loop (`calloop`) driving N `Screen`s (one per
Wayland output). `Shell` (in `src/main.rs`) owns all Wayland/SCTK state; its SCTK
handlers live in `src/output.rs` (compositor, output, layer), `src/seat.rs` (seat,
pointer) and `src/dnd.rs` (data device). Per output it creates a `Top`-layer surface anchored to the
bottom/left/right edges with a 60px exclusive zone. The surface is taller than that:
the headroom above the bar is transparent and click-through, and islands grow into it.

### Rendering is event-driven, not a render loop
There is no redraw flag on `UIState`. Each `Screen` tracks `needs_redraw` and
`frame_pending`; `Shell::request_redraw(idx)` sets `needs_redraw = true` and
draws immediately unless a `wl_surface.frame` callback is still outstanding
(`frame_pending`), in which case the frame callback (`CompositorHandler::frame`)
redraws once it fires. `Shell::draw` sets `frame_pending = true`, wraps the SHM
buffer's pixels directly with Skia (`surfaces::wrap_pixels` — no GPU surface),
calls `screen.ui.draw(canvas, &state, &font, width)`, sets `needs_redraw` to whether that
returned `Animating`, requests a new frame callback, and commits.

Component handlers report whether a redraw is needed via `Redraw { None, Now,
Animating }` (`src/ui.rs`); `UserInterface` folds every component's result with
`Redraw::max`. `Now` just means "something changed"; `Animating` returned from
`draw` makes that screen's next frame callback repaint it — this is how a running
`Animation` keeps repainting itself every frame without any handler having to
reschedule it explicitly. A `CLOCK_TICK` calloop timer in `main()` also repaints
every screen once a second and expires notifications.

### Islands (Dynamic Island-style UI)
`UserInterface::draw` paints the bar and every visible island as a single silhouette
(`ui_core::island`). Each component has its own `IslandMorph` and its content scales along with it. A component bids by
returning an `IslandRequest` from `Component::island`; the highest `priority` wins the one main island, and the
others collapse in place meanwhile, so two can show at once while one opens as another closes.
The owner draws its content in `draw_island` and receives pointer events in
`on_island_cursor`. `Shell::update_input_region` limits input to the bar and island.
See [docs/components.md](docs/components.md).

### The five async input channels (wired in `main()`)
Background threads/providers feed the calloop event loop through
`calloop::channel`s. `main()` registers each channel with `on_msg`; each closure
calls `shell.dispatch`, which fans the message out to every `Screen` and calls
`shell.request_redraw(idx)` for those that returned non-`Redraw::None`:

| Channel payload | Handler |
|---|---|
| `NotificationEvent` | `screen.ui.on_notification` |
| `(PlayerState, mpris::Event)` | `screen.ui.on_mpris` |
| `IPCEvent` | `screen.ui.on_ipc` |
| `KDEConnectEvent` | `screen.ui.on_kde_connect_event` |
| `WarpStatus` | `screen.ui.on_warp` |

`SystemStatus` (`src/system.rs`) has its own channel too: its closure updates
`UIState.system` and repaints every screen, since the toolbox always shows it. So does
player art (`src/dbus/player_art.rs`), which updates `UIState.player_art`.

A sixth channel, `UiEvent` (`RequestRedrawAll`/`AnimationUpdated`/`DismissNotification`/`ClearNotifications`), drives `Shell` directly rather than going through a
`Component` method.

`UserInterface` (`src/ui.rs`) holds `Vec<Box<dyn Component>>`; every `on_*` handler
fans out to every `Component`. `Shell` holds the one `UIState` shared by all screens
(currently `players: HashMap<String, PlayerState>`, `warp: Option<WarpStatus>` and
`notifications: NotificationStore`),
updated in the channel closures in `main()` before dispatching. **All drawing lives in components** — `UserInterface::draw`
only paints the bar and island background and calls each
`Component::draw`/`draw_island`; there is no inline text drawing left in `main.rs`/`ui.rs`.

### Window-manager IPC abstraction (`src/ipc/`)
`ipc::listen` selects a backend at runtime from `$XDG_CURRENT_DESKTOP` (`niri` or
`hyprland`). Each backend spawns a listener thread translating compositor-native events into the small internal
`IPCEvent` enum (`src/ipc/events.rs`) sent over the channel.
- **niri** (`niri.rs`): connects a `niri-ipc` socket for the `EventStream`. Maintains a `HashMap<id, Window>` to
  resolve titles on focus changes. This is the fleshed-out backend.
- **hyprland** (`hyprland.rs`): all handlers are registered but empty, and
  `start_listener` blocks its caller. Parked stub.

### Fonts (`ui_core/src/font/`)
`FontBook` (moved out of the bar into `ui_core`) wraps a Skia `FontMgr` plus a
`HashMap<String, Typeface>`. `register(key, family, style)` resolves a system
font by name via `legacy_make_typeface` ("Roboto", "Noto Sans CJK JP") and
panics if the family can't be resolved. `sized(key, px)` produces a `Font` —
**it panics if `key` was never registered** (`unwrap_or_else(|| panic!(..))`),
there is no silent fallback. Register every key you use before the first draw.

## Status / where work is happening

`TODO.md` tracks intended providers (audio, battery, network, monitor) and IPC
event-mapping work. Current live state: niri IPC handles workspace + window-focus
changes; MPRIS events arrive and update `UIState.players`. Hyprland IPC is a parked
stub. The M3 widget stack (`ui_widget`/`m3_widget`) is under active
development against the `m3_test` gallery.
