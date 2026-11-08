# Configuration

The bar reads two TOML files, both hot-reloaded while it runs:

| File | What it sets |
|------|--------------|
| `theme.toml` | Colors: the Material 3 palette for light and dark mode |
| `animation.toml` | Easing curves for animations |

Fonts, the bar's height and the set of components are not configurable yet;
they are set in code.

## Where the files go

Put your files in `$XDG_CONFIG_HOME/my_shell/` (`~/.config/my_shell/` when
`XDG_CONFIG_HOME` is unset):

```sh
mkdir -p ~/.config/my_shell
cp example/theme.toml example/animation.toml ~/.config/my_shell/
```

Each file is looked up on its own. If `~/.config/my_shell/<name>` does not
exist, the bar falls back to `example/<name>` in the source checkout, so you can
override one file and keep the other's defaults.

- The lookup happens once at startup. A file you create while the bar is running
  is picked up only after a restart; edits to a file already in use are picked up
  live.
- The `example/` fallback points at the directory the binary was **compiled**
  from. It works for `cargo run`, but a binary built elsewhere (e.g. `nix build`)
  has no such directory, so give it your own copies of both files.

## Reloading and errors

Save a file and the bar applies it on the next frame; no restart is needed.
Editors that save by writing a new file and renaming it over the old one work
too.

Errors are logged, not shown. Run with `RUST_LOG=warn` (or `info`) to see them:

- **A file that fails to parse while the bar is running** is ignored, and the
  last good version stays in effect.
- **A file that fails to parse at startup**, or is missing or empty, falls back
  to built-in defaults. For `theme.toml` that means every color is transparent
  and the bar is invisible.
- **A missing key** takes its default silently: a missing color role is
  transparent, and a missing easing uses the built-in curve. Check for typos in
  key names if something looks wrong.

## `theme.toml`

```toml
is_dark = true

[dark]
primary = "#92d5ab"
on_primary = "#003920"
surface_container = "#1b211d"
# …

[light]
primary = "#296a48"
# …
```

- `is_dark` chooses which table is used: `[dark]` when `true`, `[light]` when
  `false`. Both tables can be present; switching modes is a one-line edit.
- Each table holds Material 3 color roles. Values are `"#RRGGBB"` or
  `"#RRGGBBAA"`.
- The bar's background is `surface_container`. Write every role:
  [`example/theme.toml`](../example/theme.toml) has the full list of 49 roles,
  from `primary` to `scrim`.

### Generating it with matugen

[`example/template.toml`](../example/template.toml) is a
[matugen](https://github.com/InioX/matugen) template that writes every role for
both modes. Add it to matugen's config (`~/.config/matugen/config.toml`):

```toml
[templates.my_shell]
input_path = "/path/to/gtk_shell/example/template.toml"
output_path = "~/.config/my_shell/theme.toml"
```

Then `matugen image <wallpaper>` regenerates the theme, and the running bar
picks it up.

## `animation.toml`

```toml
[easings]
island = "cubic-bezier(0.2, 0.0, 0.0, 1.0)"
```

Each key under `[easings]` names an animation, and its value is a CSS easing
function:

| Syntax | Example |
|--------|---------|
| Keywords | `linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out` |
| `cubic-bezier(x1, y1, x2, y2)` | `cubic-bezier(0.2, 0.0, 0.0, 1.0)` |
| `linear(stops…)` | `linear(0, 0.6 40%, 1.05 70%, 1)`: values can overshoot 1 for a spring; positions are optional |

`linear()` can express springs and bounces that `cubic-bezier` cannot. Tools such
as the [Linear() Easing Generator](https://linear-easing-generator.netlify.app/)
convert a spring into a `linear()` string.

### Reusing a curve

Define a curve once under any name, then point other names at it with
`conf::<name>`:

```toml
[easings]
my_spring = "linear(0, 0.6 40%, 1.05 70%, 1)"
my_snappy = "cubic-bezier(0.2, 0.0, 0.0, 1.0)"

island = "conf::my_spring"                   # reuse a definition
other  = "cubic-bezier(0.25, 0.1, 0.25, 1)"  # or write one inline
```

A reference can point at another reference. A reference to an undefined name,
or a cycle (`a = "conf::b"`, `b = "conf::a"`), is skipped with a warning like a
parse error.

The bar reads these names:

| Name | Animates | Default if unset |
|------|----------|------------------|
| `island` | Islands growing, shrinking and morphing | ease-out cubic |
| `media_play_pause` | The media card's play button shape and the seek bar's wave height | ease-out cubic |
| `slider_handle` | A slider's handle narrowing while pressed (toolbox and seek bar) | M3 standard |
| `slider_indicator` | A slider's value indicator fading in and out | M3 standard |
| `notification_banner` | The notification card dropping in from the top of the screen and leaving | ease-out cubic |

Any other name is free for your own definitions to reuse. Separate words with `_`,
not `.`: a bare `a.b = …` is a nested TOML table and the file fails to parse. An easing that fails to parse is skipped with
a warning, and the default applies.

---

## For developers

Each file is a struct in `src/config/` that implements `WatchableConfig`
(`src/config/watchable.rs`). `main()` loads each one with
`load_and_watch(&config_path("<name>"), ui_tx.clone())`, which returns an
`Arc<RwLock<_>>` that components share.

```rust
pub trait WatchableConfig: Default + Sized + Send + Sync + 'static {
    fn try_load(path: &Path) -> Option<Self>;
    fn is_changed(&self, new: &Self) -> bool;
    fn reload_event(&self) -> UiEvent;

    fn load(path: &Path) -> Self { .. }                          // provided
    fn load_and_watch(path: &Path, ui_tx: Sender<UiEvent>) -> Arc<RwLock<Self>> { .. } // provided
}
```

`load_and_watch` spawns a thread that watches the file's **parent directory**
with `notify`, so a rename-save that replaces the file's inode does not end the
watch. On a create or modify event for the file it calls `try_load`, and, only
if `is_changed` says so, swaps the new value in and sends `reload_event()` on the
`UiEvent` channel.

| Struct | File | `reload_event()` | `is_changed` compares |
|--------|------|------------------|-----------------------|
| `Theme` (`theme.rs`) | `theme.toml` | `RequestRedrawAll` | the whole struct |
| `AnimationConfig` (`animation.rs`) | `animation.toml` | `AnimationUpdated(easings)` | the raw strings only |

- `Theme::theme()` returns the active `&ColorTheme` (`ui_core::scheme::theme`).
  `UserInterface::draw` passes it to every `Component::draw`. `Color`
  (`ui_core::scheme::color`) parses hex via `TryFrom<String>` and converts into
  `skia_safe::Color4f`.
- `AnimationConfig` keeps both the raw strings and the `Easing`s parsed by
  `ui_core::animation::parser::css_to_easing`, after `resolve` has followed any
  `conf::` references. Every name gets its own `Easing`, so a component never
  needs to know whether its name was an alias. On `AnimationUpdated`, `main()`
  calls `UserInterface::on_easing_updated(id, easing)` for each pair, which
  fans out to `Component::on_easing_updated`, so an in-flight `Animation` can
  swap its curve without a restart. `UserInterface` itself handles `"island"`
  (`ISLAND_EASING_ID` in `src/ui.rs`).

### Adding a config file

1. Add a struct in `src/config/` and implement the three required methods.
   `read_toml` in `watchable.rs` implements `try_load` for any `Deserialize` type.
   Give it `#[serde(default)]` so a partial file still loads.
2. Load it in `main()` next to the others:
   `YourConfig::load_and_watch(&config_path("your.toml"), ui_tx.clone())`.
3. Add `example/your.toml` so a fresh checkout runs without a user copy.
4. Document it in the user-facing part of this file.

### Adding an easing name

Start the animation with a built-in curve and swap it in
`Component::on_easing_updated` when `id` matches your name. `UserInterface::new`
replays every easing from the file through `on_easing_updated` once the
components exist, so the same handler covers startup and reloads; there is no
separate lookup. A `ToolboxItem` gets the same call from `Toolbox`, and
`components::update_slider_easing` handles the slider names for any `Slider`.
Add the name to the table in [`animation.toml`](#animationtoml) above.
