# Agent quick-reference

See [CLAUDE.md](../CLAUDE.md) for build/run commands and high-level architecture.
This file is the starting point for agents working in this repo.

## Commit messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):
`<type>(<scope>): <description>` — lowercase, no trailing period.
Common types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`.
Scopes mirror the module path: `feat(ipc): …`, `fix(ui): …`, `refactor(font): …`.

## Key files

| Path | Purpose |
|------|---------|
| `src/main.rs` | Event loop wiring; `Shell`; `Shell::draw`; all SCTK delegates |
| `src/ui.rs` | `Component` trait; `UserInterface`; `UIState`; `UiEvent` |
| `src/ipc/mod.rs` | `WindowManagerIPC` dispatcher; `IpcTrait` |
| `src/ipc/events.rs` | `IPCEvent` enum |
| `src/ipc/niri.rs` | Niri backend (implemented) |
| `src/ipc/hyprland.rs` | Hyprland backend (stub — all `todo!()`) |
| `src/dbus/mpris.rs` | `MprisClient` background thread; `PlayerState` |
| `src/components/clock.rs` | Reference `Component` implementation |
| `src/config/config.rs` | `Configuration` + hot-reload |
| `src/font/mod.rs` | `FontBook`; `sized(key, px)` |

## Detailed how-to guides

| Topic | Guide |
|-------|-------|
| Adding a Component | [docs/components.md](components.md) |
| Extending IPC events | [docs/ipc.md](ipc.md) |
| Adding a data source (audio, battery…) | [docs/data-sources.md](data-sources.md) |
| Configuration & UIState | [docs/config.md](config.md) |

## Known sharp edges

- **`shoud_redraw` does not exist** — CLAUDE.md mentions it but it was removed.
  `Shell::draw` renders unconditionally when called. Do not add this field
  without updating the draw path.
- **Hyprland IPC panics** — every `IpcTrait` method in `src/ipc/hyprland.rs` is
  `todo!()`. They are called automatically from `WindowManagerIPC` when
  `XDG_CURRENT_DESKTOP=hyprland`. Implement before relying on them.
- **`on_mpris` must not panic** — `UserInterface::on_mpris` fans out to every
  component for every player event. Empty bodies are fine; `todo!()` is not.
- **Config path is hardcoded** — `main.rs:164` points to an absolute path under
  `/home/minearchive/`. Move to a CLI arg or env var before shipping.
- **Fonts must be pre-registered** — `fonts.sized` with an unknown key silently
  returns a fallback. Register in `main.rs` or via `UiEvent::RegisterFont`.
- **Build requires devenv** — `skia-bindings` needs env vars from `devenv.nix`
  (`LIBCLANG_PATH`, `LD_LIBRARY_PATH`, EGL). `cargo build` outside the devenv
  shell will fail. direnv auto-activates this inside the repo.
