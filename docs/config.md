# Configuration

The bar reads two TOML files, both hot-reloaded while it runs:

| File | What it sets |
|------|--------------|
| `theme.toml` | Colors: the Material 3 palette for light and dark mode |
| `animation.toml` | Easing curves for animations |

Fonts, the bar's height and the set of components are not configurable yet;
they are set in code.

## Where the files go

Put your files in `$XDG_CONFIG_HOME/my_shell/`, or any directory given with
`--config-directory` (`-c`), (`~/.config/my_shell/` when
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
- `example/theme.toml` is gitignored: a fresh checkout has only `template.toml`, so generate
  or copy a theme first.

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
- The bar's background is `surface_container`. The roles are also mapped onto the Material 3
  Expressive widgets' scheme (the toolbox's sliders, switches and buttons). Write every role:
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
| `media_play_pause` | The media card's play button shape | ease-out cubic |
| `notification_banner` | The notification card dropping in from the top of the screen and leaving | ease-out cubic |

Any other name is free for your own definitions to reuse. Separate words with `_`,
not `.`: a bare `a.b = …` is a nested TOML table and the file fails to parse. An easing that fails to parse is skipped with
a warning, and the default applies.

---

## For developers

Each file is a struct in `src/config/` that implements `WatchableConfig`
(`src/config/watchable.rs`):

```rust
pub trait WatchableConfig: Clone + Default + Sized + Send + 'static {
    fn try_load(path: &Path) -> Option<Self>;
    fn is_changed(&self, new: &Self) -> bool;

    fn load(path: &Path) -> Self { .. }                                              // provided
    fn load_and_watch(path: &Path, on_reload: impl Fn(Self) + Send + 'static) -> Self { .. } // provided
}
```

`load_and_watch` loads the file, then watches its **parent directory** with `notify` on a
thread, so a rename-save that replaces the file's inode does not end the watch. Each version
that `is_changed` says differs goes to `on_reload`. `State::start` passes a `ui_channel`
sender there, whose closure sets the `theme` / `animation` signal on the UI thread.

| Struct | File | Signal | `is_changed` compares |
|--------|------|--------|-----------------------|
| `Theme` (`theme.rs`) | `theme.toml` | `state.theme` | the whole struct |
| `AnimationConfig` (`animation.rs`) | `animation.toml` | `state.animation` | the raw strings only |

- `Theme::theme()` returns the active `&ColorTheme` (`ui_core::scheme`). Views read colors
  with `ui::palette(state, |t| t.role)`; `main.rs` maps the theme onto md3e's scheme with
  `ui::md3e_scheme` in an effect, so a reload recolors both.
- `AnimationConfig::easings()` holds every name's `Easing`, parsed by
  `ui_core::animation::parser::css_to_easing` after `conf::` references are followed, so a
  reader never needs to know whether its name was an alias.

### Adding a config file

1. Add a struct in `src/config/` and implement the two required methods. `read_toml` in
   `watchable.rs` implements `try_load` for any `Deserialize` type. Give it
   `#[serde(default)]` so a partial file still loads.
2. Load it in `State::start` the way `theme` is, into a new signal.
3. Add `example/your.toml` so a fresh checkout runs without a user copy.
4. Document it in the user-facing part of this file.

### Adding an easing name

Use `ui::eased(state, "your_name", duration, target)`: it follows `state.animation` in an
effect and swaps the curve in, even mid-motion. An
`IslandMorph` takes its easing with `set_easing`, as `ui::bar` and the banner do in an
effect. Add the name to the table in [`animation.toml`](#animationtoml) above.
