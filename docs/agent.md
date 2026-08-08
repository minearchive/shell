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
| `src/main.rs` | Event loop wiring (`main`); `Shell`; `Shell::draw`/`request_redraw`; all SCTK delegates |
| `src/ui.rs` | `Component` trait; `Redraw` enum; `UserInterface`; `UIState`; `UiEvent` |
| `src/ipc/mod.rs` | `WindowManagerIPC` dispatcher; `IpcTrait` |
| `src/ipc/events.rs` | `IPCEvent` enum |
| `src/ipc/niri.rs` | Niri backend (implemented) |
| `src/ipc/hyprland.rs` | Hyprland backend (stub — all `todo!()`) |
| `src/dbus/mpris.rs` | `MprisClient` background thread; `PlayerState` |
| `src/dbus/warp.rs`, `src/dbus/kdeconnect/`, `src/dbus/notification.rs` | Other D-Bus providers |
| `src/components/clock.rs` | Reference `Component` implementation |
| `src/config/config.rs`, `theme.rs`, `animation/mod.rs`, `watchable.rs` | `Configuration`, `Theme`, `AnimationConfig` + hot-reload |

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
| Configuration, Theme, AnimationConfig, FontBook | [docs/config.md](config.md) |

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
  causes `UserInterface::draw` to send `UiEvent::RequestRedrawAll`. See
  CLAUDE.md's "Rendering is event-driven" section.
- **Hyprland IPC panics** — every `IpcTrait` method in `src/ipc/hyprland.rs` is
  `todo!()`. They are called automatically from `WindowManagerIPC` when
  `XDG_CURRENT_DESKTOP=hyprland`. Implement before relying on them.
- **`on_*` methods must not `todo!()`** — every `Component::on_*` handler is
  called unconditionally as events arrive; an unimplemented body must return
  the default `Redraw::None`, not panic. `on_mpris` is fully implemented today
  (`src/ui.rs`), contrary to older notes claiming otherwise.
- **Config/theme/animation paths are hardcoded** — `main.rs` points at absolute
  paths under `/home/minearchive/project/gtk_shell/example/` for
  `config.toml`, `theme.toml`, `animation.toml`. Move to a CLI arg or env var
  before shipping.
- **Fonts must be pre-registered** — `FontBook::sized` (`ui_core/src/font/mod.rs`)
  panics on an unknown key; there is no silent fallback. Register in `main.rs`
  or via `UiEvent::RegisterFont` before the first draw that uses the key.
- **Build requires devenv** — `skia-bindings` needs env vars from `devenv.nix`
  (`LIBCLANG_PATH`, `LD_LIBRARY_PATH`, EGL). `cargo build` outside the devenv
  shell will fail. direnv auto-activates this inside the repo.
