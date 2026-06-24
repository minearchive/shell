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

The app is one event loop (`calloop`) driving N `Screen`s (one per Wayland
output). `Shell` (in `src/main.rs`) owns all Wayland/SCTK state and implements
every SCTK delegate (compositor, output, layer, seat, keyboard, pointer, shm).
Per output it creates a `Top`-layer surface anchored to the bottom edge with a
60px exclusive zone.

### Rendering is event-driven, not a render loop
`UIState.shoud_redraw` (note the spelling) gates drawing. `Shell::draw` early-returns
unless `should_redraw()` returns true, which also resets the flag. Anything that
should cause a repaint must (1) set `shoud_redraw = true` and (2) send
`UiEvent::RequestRedraw(idx)` on the UI channel, which calls `Shell::draw` from
the loop. Skia draws by wrapping the SHM buffer's pixels directly
(`surfaces::wrap_pixels`) — no GPU surface.

### The three async input channels (set up per-output in `new_output`)
Background threads feed the calloop event loop through `calloop::channel`s, each
routed to a specific screen index (`c`):
- **UI redraw** → `UiEvent` → `Shell::draw`
- **Window-manager IPC** → `IPCEvent` → `screen.ui.on_ipc`
- **MPRIS / D-Bus** → `mpris::Event` → `screen.ui.on_mpris`

`UserInterface` (`src/ui.rs`) holds `Vec<Box<dyn Component>>` plus a `UIState`.
Each input handler both fans out to every `Component` (via the `Component` trait:
`draw`/`on_cursor`/`on_key`/`on_ipc`/`on_mpris`) and updates `UIState`. The
current `draw` also renders `workspace_id`/`window_title` text directly, outside
any component — components are the intended extension point but most UI is still
inline.

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

### Fonts (`src/font/`)
`Fonts` enum wraps a `FontInstance` (Skia `FontMgr` + cached `Typeface`).
`legacy_make_typeface` resolves a system font by name ("Roboto",
"Noto Sans CJK JP"). `sized(px)` produces a `Font` for drawing.

## Commit messages

Follow [Conventional Commits](https://www.conventionalcommits.org/): write each
message as `<type>(<optional scope>): <description>`, lowercase description, no
trailing period. Common types here: `feat`, `fix`, `refactor`, `docs`, `test`,
`chore`. Use scopes that match the module being touched, e.g.
`feat(ipc): map niri workspace events`, `fix(ui): reset redraw flag`,
`refactor(font): cache typeface`.

## Status / where work is happening

`TODO.md` tracks intended providers (audio, battery, network, monitor) and IPC
event-mapping work. Current live state: niri IPC handles workspace + window-focus
changes; MPRIS events arrive but `UserInterface::on_mpris` is a wall of `todo!()`
(matched but unhandled — calling it will panic). Hyprland IPC is a stub.
