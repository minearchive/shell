# Configuration, Theme, AnimationConfig, FontBook

Three separate hot-reloadable TOML-backed structs, all in `src/config/`, plus
`ui_core`'s `FontBook`. They are loaded independently in `main()` and each
lives behind its own `Arc<RwLock<_>>`, cloned into whatever component needs it.

## The `WatchableConfig` trait (`src/config/watchable.rs`)

All three configs implement this shared trait:

```rust
pub trait WatchableConfig: Default + Sized + Send + Sync + 'static {
    fn try_load(path: &str) -> Option<Self>;
    fn write(&self, path: &str) -> io::Result<()>;
    fn is_changed(&self, new: &Self) -> bool;
    fn reload_event(&self) -> UiEvent;

    fn load(path: &str) -> Self { .. }                          // provided
    fn load_and_watch(path: &str, ui_tx: Sender<UiEvent>) -> Arc<RwLock<Self>> { .. } // provided
}
```

`load_and_watch` spawns a thread with a `notify::recommended_watcher` polling
the file every 100ms; on a modify event it calls `try_load`, compares against
the current value with `is_changed`, and — only if it actually changed —
swaps the `RwLock` contents and sends `reload_event()` on the `UiEvent`
channel. This is how config edits reach the running bar without a restart.

To add a fourth watchable config, implement these four methods for your type
and call `YourType::load_and_watch(path, ui_tx.clone())` in `main()` next to
the other three.

## `Configuration` (`src/config/config.rs`)

```rust
#[derive(Deserialize, Serialize, Default)]
pub struct Configuration {
    pub name: String,
}
```

That's the whole struct today — **it does not hold color/theme fields**; those
live on `Theme` (below). `reload_event()` returns `UiEvent::RequestRedrawAll`.

**Path** — hardcoded in `main()`:
```
/home/minearchive/project/gtk_shell/example/config.toml
```
This should become a CLI argument or environment variable before shipping.

## `Theme` (`src/config/theme.rs`)

```rust
#[derive(Deserialize, Serialize, Default)]
pub struct Theme {
    pub is_dark: bool,
    pub dark: ColorTheme,
    pub light: ColorTheme,
}
```

`theme.theme()` returns `&self.dark` or `&self.light` per `is_dark`. Path:
`example/theme.toml`, same hardcoded-absolute-path caveat as above.
`reload_event()` also returns `UiEvent::RequestRedrawAll`.

`ColorTheme` (`ui_core::scheme::theme`) is the full Material 3 color-role set
— 45 fields (`primary`, `on_primary`, `primary_container`, …, `surface`,
`outline`, `scrim`, etc.), each a `Color` (`ui_core::scheme::color`) parsed
from a hex string (`#RRGGBB` / `#RRGGBBAA`) via `TryFrom<String>`. `Color`
converts `Into<skia_safe::Color4f>` for direct use in Skia paints, and has
`with_alpha`/`lerp`.

### Example theme TOML

```toml
is_dark = true

[dark]
primary = "#D0BCFF"
on_primary = "#381E72"
surface_container = "#211F26"
# … the remaining ColorTheme roles

[light]
primary = "#6750A4"
# …
```

`UserInterface::draw` reads the theme through `Arc<RwLock<Theme>>`, calls
`.theme()` to get the active `&ColorTheme`, clears the canvas to
`theme.surface_container`, then passes `theme` to every `Component::draw`.

## `AnimationConfig` (`src/config/animation/mod.rs`)

```rust
pub struct AnimationConfig {
    raw: HashMap<String, String>,       // name -> CSS timing-function source
    easings: HashMap<String, Easing>,   // name -> parsed Easing
}
```

Parses each value with `ui_core::animation::parser::css_to_easing` (standard
CSS `cubic-bezier(...)`/keyword syntax) from `example/animation.toml`, whose
raw shape is `RawAnimationConfig { easings: HashMap<String, String> }`
(`src/config/animation/easings.rs`). `add_easing`/`get_easing` mutate/read the
parsed map at runtime.

**Different from `Configuration`/`Theme`**: `AnimationConfig::reload_event()`
returns `UiEvent::AnimationUpdated(self.easings.clone())`, not
`RequestRedrawAll`. `main()`'s `UiEvent` handler forwards each `(id, easing)`
pair to every screen's `UserInterface::on_easing_updated`, which fans out to
`Component::on_easing_updated` — this lets a component (e.g. `Clock`, keyed
on `"a"`) swap the `Easing` an in-flight `Animation` uses without restarting
the app. `is_changed` compares only `raw`, so re-saving a file with the same
easing strings doesn't trigger a spurious broadcast.

### Example animation TOML

```toml
[easings]
a = "cubic-bezier(0.2, 0.0, 0.0, 1.0)"
```

## `UIState` (`src/ui.rs`)

`UIState` is the shared render snapshot passed by reference to every
`Component::draw` call. Components **read** it; they never write to it
directly. Only `UserInterface` updates it, inside the `on_*` handlers (e.g.
`on_mpris` updates `players`, `on_warp` updates `warp`).

```rust
pub struct UIState {
    pub players: HashMap<String, PlayerState>,
    pub warp: Option<WarpStatus>,
}
```

| Field | Type | Updated by | Notes |
|-------|------|-----------|-------|
| `players` | `HashMap<String, PlayerState>` | `on_mpris` | Keyed by player identity; inactive players (`state.active == false`) are removed |
| `warp` | `Option<WarpStatus>` | `on_warp` | Latest Cloudflare WARP status, if any |

### Adding a field

1. Add the public field to `UIState` in `src/ui.rs`.
2. Initialise it in `UIState::new`.
3. Update it in the relevant `UserInterface::on_*` method. Sending
   `UiEvent::RequestRedraw`/`RequestRedrawAll` is the *handler's* job, driven
   by that handler's `Redraw` return value — see
   [docs/components.md](components.md) and [docs/data-sources.md](data-sources.md).

## `FontBook` (`ui_core/src/font/mod.rs`)

Moved out of the bar into `ui_core` so `m3_widget`/`m3_test` can share it.
Maps short string keys to `Typeface` instances.

**Registration** — done once at startup in `main()` or lazily via
`UiEvent::RegisterFont(key, family, style)`:
```rust
book.register("noto_sans", "Noto Sans CJK JP", FontStyle::normal());
```
`register` itself panics if the Skia `FontMgr` can't resolve `family`.

**Usage in draw** — `fonts.sized("noto_sans", 32.)` returns a `skia_safe::Font`
sized to 32px. **An unknown key panics** (`unwrap_or_else(|| panic!(...))`) —
there is no silent fallback. Always register a key before any `draw` path can
reach `sized(key, ..)` with it.
