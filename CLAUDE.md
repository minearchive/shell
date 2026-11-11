# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Wayland desktop bar/shell (`wlr-layer-shell`) built on
[AntiWebUI](https://github.com/bea4dev/AntiWebUI): a reactive Rust UI library that renders
with wgpu and opens layer-shell surfaces itself. Despite the crate name in old docs
(`gtk_learn`) and the directory name (`gtk_shell`), there is no GTK, and no Skia any more.

AntiWebUI and its Material 3 Expressive widgets (`anti-webui-md3e`) are git dependencies
pinned to one rev in the root `Cargo.toml`. The library changes daily, so upgrading the rev
is a deliberate step: update both entries and the `outputHashes` entry in `flake.nix`.

## Build & run

The development environment is `devShells.default` in **flake.nix**, pinned by
`flake.lock`. Enable Nix's `nix-command` and `flakes` experimental features, then
enter it with `nix develop`. With direnv installed and hooked into your shell,
run `direnv allow` once; `.envrc` uses `use flake` to load the same environment.

The shell provides Rust/Cargo, Clippy, rustfmt, rust-analyzer, the native libraries the
build links (dbus, fontconfig, wayland, libxkbcommon, libxcb), and an `LD_LIBRARY_PATH`
with what is loaded at run time: vulkan-loader (wgpu), mesa/libGL (wgpu's GL fallback) and
pipewire (AntiWebUI's audio, for the volume). Running the bar requires a Wayland session
with layer-shell.

```sh
nix develop
cargo build
RUST_LOG=debug cargo run      # env_logger reads RUST_LOG; use info/debug
cargo run -- -c ./example      # --config-directory: read theme.toml etc. from there
cargo run -- action launcher   # send an action to the running bar
cargo test --workspace
cargo test test_run -- --ignored --nocapture   # mpris event stream (needs a live MPRIS player, runs until it quits)
```

For a non-interactive build (also used by CI):

```sh
nix develop --command cargo build --locked
```

`nix build` builds the `default` package and wraps the binary with the same run-time
library path. Day-to-day development uses the devShell and Cargo.

At startup the Vulkan loader may log ERRORs about the system's Mesa radeon/lvp ICDs needing
a newer glibc than the devShell's. They are harmless while another ICD (Intel here) loads.

## Architecture

A Cargo workspace of the bar binary (`my_shell`, `src/`) and `ui_core`, what is left of the
old Skia widget toolkit: `LayoutRect`, the theme's `ColorTheme`/`Color`, CSS easing parsing
and `Animation`, `IslandMorph`, and `elide`/`wrap_text`. It has no rendering of its own.

### Reactivity, not a render loop

`main()` runs `Shell::new().namespace("my_shell").run(..)`. Inside it, `State::start`
(`src/state.rs`) starts every provider and returns `State`: a bundle of AntiWebUI
`Signal`s (`players`, `player_art`, `notifications`, `system`, `warp`, `theme`,
`animation`, `action`, …) plus `commands`, the senders to the providers.

Providers keep their own threads (D-Bus, niri IPC, polling) and send to the UI thread
through `ui_channel`s, whose closures update the signals. Views read signals in
`dyn_style`/`dyn_view`/`text(|| ..)` closures and `effect`s, and AntiWebUI repaints what
depends on what changed. There is no redraw flag. Running animations step with
`request_frame` or AntiWebUI's `animated`.

The volume and brightness come from AntiWebUI's `shell-audio` (PipeWire events) and
`shell-session` (inotify on the backlight) features, not polling. The rest of
`SystemStatus` (`src/system.rs`) is polled every 5s.

### Surfaces

`for_each_output` opens, per output:

- the bar: a `Layer::Top` surface anchored bottom/left/right, `BAR_HEIGHT` (48px) plus
  `ISLAND_MAX_HEIGHT` of transparent headroom tall, with a `BAR_HEIGHT` exclusive zone. Its
  input region follows the bar and the open island.
- the outside-press catcher: a `Layer::Top` surface kept at 1×1 with no input while no
  island is open, and stretched over the whole output, minus the bar's input region, while
  one is. A press on it closes every island (`catch_outside_presses` in `main.rs`).
- the notification banner: a `Layer::Overlay` surface along the top edge
  (`components::notifications::banner`). It is not an island.

**AntiWebUI builds a wgpu instance and device per surface**, so opening one costs a visible
lag. Open surfaces once and reshape them (`set_size`, `set_anchor`, `set_input_region`);
never open one per use. When shrinking, set the size before dropping anchors: a stretched
size without both opposite anchors is a protocol error.

### Parts and islands (`src/ui.rs`)

Each component is a function returning a `Part`: what it puts in the bar (`BarSlot` +
`View`), an optional `IslandPart` (a reactive `IslandRequest` bid, the content view, a
`close`, and a `frame` signal the bar writes), and an optional `overlay` drawn over the bar
and islands from the part's place in the bar. `ui::bar` lays the parts out with flex,
computes each part's x itself (AntiWebUI has no API to ask an element's position), draws
the bar and every visible island as one `svg::path` silhouette (`island_path`, kurbo), and
morphs each island with its own `IslandMorph`. The highest `priority` wins the main island;
the others collapse where they are, so one can open while another closes.

Opening happens on press (`on_pointer_down`), not click. A press on a part closes every
other part's island first. See [docs/components.md](docs/components.md).

### Async inputs

| Provider | Module | Feeds |
|---|---|---|
| Notification server | `src/dbus/notification.rs` | `notifications` |
| MPRIS players + polled positions | `src/dbus/mpris.rs` | `players`, `last_playing` |
| Cover art (polled) | `src/dbus/player_art.rs` | `player_art` |
| Compositor IPC | `src/ipc/` | `focused_title`, `focused_workspace` |
| KDE Connect | `src/dbus/kdeconnect/` | logged only |
| Cloudflare WARP | `src/dbus/warp.rs` | `warp` |
| System status (5s poll) | `src/system.rs` | `system` |
| CLI actions | `src/cli.rs` | `action` |
| Config files | `src/config/` | `theme`, `animation` |

`src/cli.rs` holds the clap CLI. `my_shell action <name>` connects to
`$XDG_RUNTIME_DIR/my_shell.sock`, writes the `Action` as one JSON line and waits for `ok`;
the bar binds that socket at startup and refuses to start if another instance answers on
it. `action` carries a counter, so the same action twice still reruns its readers.

A tick thread expires notifications every second.

### Window-manager IPC (`src/ipc/`)

`ipc::listen` picks a backend from `$XDG_CURRENT_DESKTOP`. niri (`niri.rs`) translates the
event stream into `IPCEvent`. Hyprland (`hyprland.rs`) is a parked stub.

## Status

`TODO.md` tracks the AntiWebUI migration phase by phase, including what still needs to be
checked on screen.
