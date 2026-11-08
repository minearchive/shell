# Agent quick-reference

See [CLAUDE.md](../CLAUDE.md) for build/run commands and high-level architecture.
This file is the starting point for agents working in this repo.

## Commit messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):
`<type>(<scope>): <description>` — lowercase, no trailing period.
Common types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`.
Scopes mirror the module path: `feat(ipc): …`, `fix(ui): …`, `refactor(font): …`.

## Key files

The bar binary (`my_shell`, `src/`):

| Path | Purpose |
|------|---------|
| `src/main.rs` | Event loop wiring (`main`); `Shell`; `Shell::draw`/`request_redraw` |
| `src/output.rs`, `src/seat.rs`, `src/dnd.rs` | SCTK handlers: outputs and layer surfaces; seat and pointer; drag-and-drop |
| `src/catcher.rs` | Transparent full-screen surface that detects presses outside the bar |
| `src/ui.rs` | `Component` trait; `Redraw` enum; `UserInterface`; `UIState`; `UiEvent` |
| `src/ipc/mod.rs` | `listen`: picks the backend from `$XDG_CURRENT_DESKTOP` |
| `src/ipc/events.rs` | `IPCEvent` enum |
| `src/ipc/niri.rs` | Niri backend (implemented) |
| `src/ipc/hyprland.rs` | Hyprland backend (stub — empty handlers) |
| `src/dbus/mpris.rs` | `start_listener` background thread; `PlayerState` |
| `src/dbus/warp.rs`, `src/dbus/kdeconnect/`, `src/dbus/notification.rs` | Other D-Bus providers |
| `src/components/clock.rs` | Reference `Component` implementation |
| `src/components/notifications.rs` | Reference island implementation |
| `src/components/toolbox/` | Status icons and their detail panel; add entries in `items.rs` |
| `src/system.rs` | `SystemStatus`: polls network, Bluetooth, audio, backlight, battery |
| `src/config/mod.rs`, `theme.rs`, `animation.rs`, `watchable.rs` | `config_path`, `Theme`, `AnimationConfig` + hot-reload |

The workspace widget crates (not yet used by the bar — see
[docs/widgets.md](widgets.md)):

| Path | Purpose |
|------|---------|
| `ui_core/src/font/mod.rs` | `FontBook`; `sized(key, px)` (panics on unknown key) |
| `ui_core/src/geometry.rs` | `LayoutRect`, `Size` |
| `ui_widget/src/widget.rs` | The `Widget` trait — distinct from `Component` above |
| `m3_widget/src/` | Material 3 widget implementations |
| `m3_test/src/main.rs` | `cargo run -p m3_test` — the widget dev/demo gallery |

## Detailed how-to guides

| Topic | Guide |
|-------|-------|
| Adding a bar Component | [docs/components.md](components.md) |
| The ui_core/ui_widget/m3_widget stack, `Widget` trait | [docs/widgets.md](widgets.md) |
| Extending IPC events | [docs/ipc.md](ipc.md) |
| Adding a data source (audio, battery…) | [docs/data-sources.md](data-sources.md) |
| Config files (`theme.toml`, `animation.toml`), `WatchableConfig` | [docs/config.md](config.md) |
| `UIState`, `FontBook` | [docs/components.md](components.md) |

## Known sharp edges

- **`Component` (bar) vs `Widget` (m3 stack) are different traits** — a new
  agent's most common mistake in this repo. `Component::draw` takes
  `(canvas, &UIState, &FontBook, &ColorTheme)` and returns `Redraw`; `Widget::draw`
  takes `(canvas, &ColorTheme, &FontBook)` and returns `bool`. They are not
  related by any supertrait and nothing currently bridges them — the bar does
  not construct any `m3_widget` type. See [docs/widgets.md](widgets.md).
- **No `shoud_redraw` / `UIState` redraw flag** — redraws are driven by
  `Screen.needs_redraw`/`frame_pending` plus each handler's `Redraw` return
  value (`None`/`Now`/`Animating`), folded with `Redraw::max`. `Animating`
  keeps that screen's `needs_redraw` set across `Shell::draw`. See
  CLAUDE.md's "Rendering is event-driven" section.
- **Hyprland IPC is a parked stub** — `HyprlandIpc::new` registers empty handlers
  and `start_listener` blocks its caller, so it must move to a thread before use.
- **`on_*` methods must not `todo!()`** — every `Component::on_*` handler is
  called unconditionally as events arrive; an unimplemented body must return
  the default `Redraw::None`, not panic.
- **Theme/animation paths** — `config_path` uses `$XDG_CONFIG_HOME/my_shell/<name>`
  (`~/.config` if unset) when that file exists, otherwise the checkout's
  `example/<name>`. The choice is made once at startup.
- **Fonts must be pre-registered** — `FontBook::sized` (`ui_core/src/font/mod.rs`)
  panics on an unknown key; there is no silent fallback. Register in `main.rs`
  before the first draw that uses the key.
- **Build inside the Nix devShell** — `skia-bindings` needs env vars from
  `flake.nix` (`LIBCLANG_PATH`, `LD_LIBRARY_PATH`, EGL). Use `nix develop`,
  or `direnv allow` for automatic activation via `.envrc`. CI uses
  `nix develop --command cargo build --locked`.
