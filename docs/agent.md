# Agent quick-reference

See [CLAUDE.md](../CLAUDE.md) for build/run commands and high-level architecture.
This file is the starting point for agents working in this repo.

## Commit messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):
`<type>(<scope>): <description>` — lowercase, no trailing period.
Common types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`.
Scopes mirror the module path: `feat(ipc): …`, `fix(ui): …`, `feat(toolbox): …`.

## Key files

| Path | Purpose |
|------|---------|
| `src/main.rs` | `Shell::run`; per output: the bar, the outside-press catcher, the banner |
| `src/state.rs` | `State`: the providers' signals and `Commands` |
| `src/ui.rs` | `Part`, `IslandPart`, `IslandRequest`, `bar()`; helpers `eased`, `measure`, `palette`, `md3e_scheme` |
| `src/components/clock.rs` | The smallest `Part` |
| `src/components/media.rs` | An island with an overlay that travels between the bar and the card |
| `src/components/notifications.rs` | The top banner (its own surface, not an island) |
| `src/components/toolbox/` | Status icons; controls (left press) and notifications (right press) panels; entries in `items.rs` |
| `src/system.rs` | `SystemStatus`: network, Bluetooth, battery, power profile (5s poll) |
| `src/dbus/` | MPRIS, cover art, notifications (a D-Bus service), WARP, KDE Connect |
| `src/ipc/` | Compositor IPC: niri, and a Hyprland stub |
| `src/config/` | `config_path`, `Theme`, `AnimationConfig`, hot reload |
| `ui_core/` | Geometry, color scheme, easing and `Animation`, `IslandMorph`, `elide`/`wrap_text` |
| `scripts/measure.sh`, `scripts/shmem.sh` | CPU / memory / GPU measurement of a running bar |

## Detailed how-to guides

| Topic | Guide |
|-------|-------|
| Adding a component, an island, a toolbox entry | [docs/components.md](components.md) |
| Extending IPC events | [docs/ipc.md](ipc.md) |
| Adding a data source | [docs/data-sources.md](data-sources.md) |
| Config files (`theme.toml`, `animation.toml`) | [docs/config.md](config.md) |

## Known sharp edges

- **Every surface costs a GPU device.** AntiWebUI creates a wgpu instance and device per
  window or layer. Open surfaces once per output and reshape them; opening one on demand
  lags visibly.
- **Layer shrinking order.** A stretched size needs both opposite edges anchored, and each
  change may be committed on its own: anchor before stretching, shrink before dropping
  anchors. The other order kills the bar with a protocol error.
- **No element positions.** AntiWebUI can't tell you where an element landed. `ui::bar`
  computes each part's x from the widths reported by `on_resized`; overlays rely on it.
- **Press, not click.** `on_click` fires on release. Things that open or close use
  `on_pointer_down`, which also lets a press on the bar close the other islands first.
- **Reading a signal in an effect subscribes to it.** Wrap writes that also read in
  `untrack`, as `State::start` does for the volume and brightness.
- **`set_timeout` needs a window.** Use it inside a surface's view, not at the `Shell::run`
  root; the notification expiry tick is a thread for that reason.
- **Hyprland IPC is a parked stub** — `HyprlandIpc::new` registers empty handlers and
  `start_listener` blocks its caller.
- **Config paths** — `config_path` uses `--config-directory` if given, else
  `$XDG_CONFIG_HOME/my_shell/<name>` (`~/.config` if unset) when that file exists, else the
  checkout's `example/<name>`. `example/theme.toml` is gitignored, so a fresh worktree has
  none and the default theme is fully transparent.
- **Build inside the Nix devShell** — it provides the native libraries and the run-time
  `LD_LIBRARY_PATH` (Vulkan, PipeWire). CI uses `nix develop --command cargo build --locked`.
