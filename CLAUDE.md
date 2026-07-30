# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Wayland desktop bar/shell (`wlr-layer-shell`) rendered with Skia. Despite the
crate name (`gtk_learn`), the directory name (`gtk_shell`), and GTK packages in
the Nix files, the running code uses **no GTK** — it talks raw Wayland through
`smithay-client-toolkit` and draws with `skia-safe` into SHM buffers. Treat the
GTK references in `flake.nix`/`devenv.nix` as stale scaffolding, not as the
architecture.

## Build & run

The dev environment is provided by **devenv** (auto-loaded via direnv / `.envrc`).
The `LIBCLANG_PATH`, `LD_LIBRARY_PATH`, and EGL env vars set in `devenv.nix` are
required for the `skia-bindings` build and for GL/Wayland at runtime — build and
run from inside the devenv shell (direnv handles this automatically in this dir).

```sh
cargo build
RUST_LOG=debug cargo run      # env_logger reads RUST_LOG; use info/debug
cargo test                    # runs the mpris event-stream test in src/dbus/mpris.rs
cargo test test_run -- --nocapture   # the single integration-ish test (needs a live MPRIS player)
```

Note `flake.nix` builds the `default` package via `rustPlatform.buildRustPackage`,
but day-to-day work uses devenv + cargo, not `nix build`.

## Architecture

This is a **Cargo workspace** of five crates plus the bar binary. The binary
(`my_shell`, root `Cargo.toml`, `src/`) is the only thing that talks Wayland;
the other four are a layered, presentation-only widget toolkit that the binary
does **not** currently depend on:

```
ui_core   → no internal deps (skia-safe, serde)      geometry, pointer/keyboard
            events, animation, color scheme, FontBook
ui_widget → ui_core                                   Widget trait, Row/Column,
                                                        ScrollableWidget
m3_widget → ui_core, ui_widget                         Material 3 widgets
m3_test   → ui_core, m3_widget (dev harness, winit)    `cargo run -p m3_test`
macros    → nothing                                    boxed!(a, b, c) -> Vec<Box<..>>
my_shell  → ui_core, macros (+ calloop, wayland, zbus, tokio, …)  the bar
```

**`my_shell` depends on `ui_core` only** — it does not yet pull in `ui_widget`
or `m3_widget`. The M3 widget stack is developed and demoed standalone via
`cargo run -p m3_test` (a winit+softbuffer gallery), not inside the bar. See
[docs/widgets.md](docs/widgets.md) for the full crate/trait reference.

**Two different, easily-confused traits exist:**
- `Component` (`src/ui.rs`, bar-only) — what the bar's UI is built from today.
- `Widget` (`ui_widget::Widget`, workspace-only) — what `m3_widget` implements.
  Not used by the bar (yet).
They are not related and do not share a supertrait. See
[docs/widgets.md](docs/widgets.md) for the distinction in detail.

The bar itself is one event loop (`calloop`) driving N `Screen`s (one per
Wayland output). `Shell` (in `src/main.rs`) owns all Wayland/SCTK state and
implements every SCTK delegate (compositor, output, layer, seat, keyboard,
pointer, shm). Per output it creates a `Top`-layer surface anchored to the
bottom/left/right edges with a 60px exclusive zone.

### Rendering is event-driven, not a render loop
There is no redraw flag on `UIState`. Each `Screen` tracks `needs_redraw` and
`frame_pending`; `Shell::request_redraw(idx)` sets `needs_redraw = true` and
draws immediately unless a `wl_surface.frame` callback is still outstanding
(`frame_pending`), in which case the frame callback (`CompositorHandler::frame`)
redraws once it fires. `Shell::draw` clears `needs_redraw`, sets
`frame_pending = true`, wraps the SHM buffer's pixels directly with Skia
(`surfaces::wrap_pixels` — no GPU surface), calls `screen.ui.draw(canvas, &font)`,
requests a new frame callback, and commits.

Component handlers report whether a redraw is needed via `Redraw { None, Now,
Animating }` (`src/ui.rs`); `UserInterface` folds every component's result with
`Redraw::max`. `Now` just means "something changed"; `Animating` additionally
makes `UserInterface::draw` send `UiEvent::RequestRedrawAll`, which redraws
every screen — this is how a running `Animation` keeps repainting itself every
frame without any handler having to reschedule it explicitly.

### The five async input channels (wired in `main()`)
Background threads/providers feed the calloop event loop through
`calloop::channel`s. `main()` registers one `insert_source` per channel; each
closure fans the message out to every `Screen`, collects which ones returned
non-`Redraw::None`, and calls `shell.request_redraw(idx)` for those:

| Channel payload | Handler |
|---|---|
| `NotificationEvent` | `screen.ui.on_notification` |
| `(PlayerState, mpris::Event)` | `screen.ui.on_mpris` |
| `IPCEvent` | `screen.ui.on_ipc` |
| `KDEConnectEvent` | `screen.ui.on_kde_connect_event` |
| `WarpStatus` | `screen.ui.on_warp` |

A sixth channel, `UiEvent` (`RequestRedraw`/`RequestRedrawAll`/`RegisterFont`/
`AnimationUpdated`), drives `Shell` directly rather than going through a
`Component` method.

`UserInterface` (`src/ui.rs`) holds `Vec<Box<dyn Component>>` plus a `UIState`
(currently `players: HashMap<String, PlayerState>` and `warp: Option<WarpStatus>`).
Every `on_*` handler both fans out to every `Component` and updates `UIState`
where relevant. **All drawing lives in components** — `UserInterface::draw`
only clears the canvas to `theme.surface_container` and calls each
`Component::draw`; there is no inline text drawing left in `main.rs`/`ui.rs`.

### Window-manager IPC abstraction (`src/ipc/`)
`WindowManagerIPC` is an enum dispatcher selected at runtime from
`$XDG_CURRENT_DESKTOP` (`niri` or `hyprland`). Both backends implement `IpcTrait`
(`get_current_window_name`, `get_current_workspace`). Each backend spawns a
listener thread translating compositor-native events into the small internal
`IPCEvent` enum (`src/ipc/events.rs`) sent over the channel.
- **niri** (`niri.rs`): connects two `niri-ipc` sockets — one for synchronous
  requests, one for the `EventStream`. Maintains a `HashMap<id, Window>` to
  resolve titles on focus changes. This is the fleshed-out backend.
- **hyprland** (`hyprland.rs`): all handlers are registered but empty;
  `IpcTrait` methods are `todo!()`. Largely a stub.

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
changes; MPRIS events arrive and `UserInterface::on_mpris` is fully implemented
(updates `UIState.players`). Hyprland IPC is a stub (`IpcTrait` methods are
`todo!()`). The M3 widget stack (`ui_widget`/`m3_widget`) is under active
development against the `m3_test` gallery and is not yet wired into the bar.
