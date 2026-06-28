# Configuration & shared render state

## Configuration (`src/config/config.rs`)

`Configuration` is a TOML file loaded at startup and hot-reloaded via `notify`
whenever the file changes.

**Loading** — call `Configuration::load_and_watch(path, ui_tx)` once in `main.rs`.
It returns an `Arc<RwLock<Configuration>>` that is cloned into every component
that needs it.

**Reading** — acquire a read lock and drop it before any blocking call:
```rust
let cfg = self.config.read().unwrap();
let color = cfg.theme().primary;
// lock is released here
```

Never hold the lock across `.await` or while acquiring another lock.

**Path** — currently hardcoded in `main.rs:164`:
```
/home/minearchive/project/gtk_shell/example/config.toml
```
This should become a CLI argument or environment variable.

### Color theme

`Configuration::theme()` returns `&self.dark` or `&self.light` based on
`self.is_dark`. Both are `ColorTheme` structs with Material You colour role
fields: `primary`, `on_primary`, `surface_container`, `outline`, etc.

Each field is a `Color` parsed from a hex string (`#RRGGBB` or `#RRGGBBAA`).
`Color` implements `Into<skia_safe::Color4f>` for direct use in Skia paints.

### Example config (TOML)

```toml
name = "material-dark"
is_dark = true

[dark]
primary = "#D0BCFF"
surface_container = "#211F26"
# … other Material You roles
```

---

## UIState (`src/ui.rs`)

`UIState` is the shared render snapshot passed by reference to every
`Component::draw` call. Components **read** it; they never write to it directly.
Only `UserInterface` updates it via `on_ipc`, `on_mpris`, and `on_cursor`.

```rust
pub struct UIState {
    pub workspace_id: String,
    pub window_title: String,
    pub players: HashMap<String, PlayerState>,
    pub padding: f32,
}
```

| Field | Updated by | Notes |
|-------|-----------|-------|
| `workspace_id` | `on_ipc` / `FocusedWorkspaceChanged` | String form of workspace number |
| `window_title` | `on_ipc` / `FocusedWindowTitleChanged` | Empty string when no window |
| `players` | `on_mpris` | Keyed by player identity; inactive players are removed |
| `padding` | `on_cursor` (horizontal scroll) | Debug/experimental |

### Adding a field

1. Add the public field to `UIState` in `src/ui.rs`.
2. Initialise it in `UIState::new`.
3. Update it in the relevant `UserInterface::on_*` method and send
   `UiEvent::RequestRedraw(self.idx)` if a repaint is needed.

---

## FontBook (`src/font/mod.rs`)

`FontBook` maps short string keys to `Typeface` instances.

**Registration** — done once at startup in `main.rs` or lazily via
`UiEvent::RegisterFont(key, family, style)`:
```rust
book.register("noto_sans", "Noto Sans CJK JP", FontStyle::normal());
```

**Usage in draw** — `fonts.sized("noto_sans", 32.)` returns a `skia_safe::Font`
sized to 32 px. An unknown key returns a silent fallback (wrong rendering, no
panic).
