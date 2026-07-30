# The M3 widget stack (`ui_core` / `ui_widget` / `m3_widget` / `m3_test`)

This is a separate, layered widget toolkit living in the workspace alongside
the bar. **The bar binary (`my_shell`) does not depend on `ui_widget` or
`m3_widget` today** — only on `ui_core`. This stack is developed and
demonstrated standalone via the `m3_test` gallery binary. Wiring it into the
bar is future work, not something already done.

If you came here looking for the bar's own UI extension point, see
[docs/components.md](components.md) instead — that's a different trait
(`Component`, `src/ui.rs`), unrelated to anything below.

## Crate layout and dependency direction

```
ui_core   (no internal deps)                serde, skia-safe, skia-bindings
  │
  ▼
ui_widget (→ ui_core)                       skia-safe, skia-bindings
  │
  ▼
m3_widget (→ ui_core, ui_widget)            + serde, skia, winit, softbuffer, toml
  │
  ▼
m3_test   (→ ui_core, m3_widget)            + winit, softbuffer, taffy, arboard
```

`macros` (just the `boxed!` macro, see below) has no dependents in this stack;
it's used by both `my_shell` and `m3_test`. Shared workspace dependency
versions (`[workspace.dependencies]` in the root `Cargo.toml`): `skia-safe`/
`skia-bindings` 0.97.2, `taffy` 0.12.2, `winit` 0.30, `softbuffer` 0.4, `serde`
1.0.228, `toml` 1.1.2.

## `ui_core` — presentation-only primitives

No layout-engine dependency (deliberately no `taffy`), no Wayland/windowing
dependency. `ui_core::lib.rs` re-exports: `animation`, `font`, `geometry`,
`keyboard`, `pointer`, `scheme`, `util`.

| Module | Contents |
|---|---|
| `geometry` | `LayoutRect { x, y, width, height: f32 }` — the single source of truth for a widget's assigned position/size, layout-engine-agnostic. `new`, `empty`, `contains(x,y)`, `inset`/`outset`, `to_skia`. `Size { width, height }` — a widget's *intrinsic* measured footprint (distinct concept: what it wants vs. where it was placed). |
| `pointer` | `type Point = (f64, f64)`; `PointerEventKind { Enter, Leave, Motion, Press{button}, Release{button}, Axis{horizontal, vertical} }`; `PointerEvent { position, kind }` with `x()`/`y()`/`button()`/`is_press()`/`is_release()`; `AxisScroll { absolute, discrete, stop }`; `button::{LEFT, RIGHT, MIDDLE}` constants. |
| `keyboard` | `KeyboardEvent { kind, modifiers }`; `KeyboardEventKind { Focus, Blur, Press{keysym,repeat}, Release{keysym}, Preedit{text,cursor}, Commit(String) }`; `Modifiers { ctrl, alt, shift, logo, caps_lock, num_lock }`; `insertable_text(&str) -> Option<String>` (filters the XKB control chars Backspace/Return/Escape/Delete map onto); `key::*` keysym constants. |
| `util` | `BoundingBox { x, y, width, height }` — ink/text-extent measurement, distinct from `LayoutRect` (allocated space). `from_text(x, y, text, font)`, `zero()`, `is_cover(x, y)`. This is what `src/components/clock.rs` uses for hit-testing, not `LayoutRect`. |
| `font` | `FontBook { typefaces: HashMap<String, Typeface> }`. `new()`, `register(key, family, style) -> &mut Self` (panics if the family can't be resolved), `sized(key, size) -> Font` — **panics if `key` was never registered**, no silent fallback. |
| `animation::animation` | `Animation<T>` (only `Animation<f32>` implemented). `new(from, to, Duration, Easing)`, `value()`, `is_done()`, `is_traveling()` (not done AND `from != to`), `reset(from,to)`, `set_target(to)`, `transition_easing(e)`, `set_easing`, `set_duration`, `from()`/`to()`. Submodule `easing` with ~30 standard curves (`ease_in/out/in_out_{sine,quad,cubic,quart,quint,expo,circ,back,elastic,bounce}`). |
| `animation::parser` | `type Easing = Arc<dyn Fn(f32) -> f32>`; `cubic_bezier(x1,y1,x2,y2) -> Easing`; `css_to_easing` parses CSS timing-function syntax (consumed by `src/config/animation`). |
| `scheme::color` | `Color { r, g, b, a: f32 }` — serde + `TryFrom<String>` hex parsing (`#RRGGBB`/`#RRGGBBAA`), `with_alpha`, `lerp`. |
| `scheme::theme` (`ColorTheme`) | The full 45-field Material 3 color-role palette (`primary`, `on_primary`, `primary_container`, … `scrim`). Serde-deserializable, all fields default to transparent. See [docs/config.md](config.md) for how the bar loads this from TOML. |

## `ui_widget` — the `Widget` trait and layout containers

`lib.rs` exports `Widget`, `Row`, `Column`, `CrossAlign`, `ScrollableWidget`.

### `Widget` — **not** the bar's `Component`

```rust
pub trait Widget {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool;
    fn on_pointer(&mut self, event: &PointerEvent) -> bool { false }
    fn measure(&self, fonts: &FontBook) -> Size { /* default: current layout_rect's size */ }
    fn on_keyboard(&mut self, event: &KeyboardEvent) -> bool { false }
    fn set_layout_rect(&mut self, rect: LayoutRect);
    fn layout_rect(&self) -> LayoutRect;
    fn hit_rect(&self) -> LayoutRect { self.layout_rect() }
    fn focusable(&self) -> bool { false }
    fn set_focused(&mut self, focused: bool) {}
}
```

This is a **different trait from `Component`** (`src/ui.rs`, the bar's UI
extension point — see [docs/components.md](components.md)). Do not confuse
them:

| | `Component` (bar, `src/ui.rs`) | `Widget` (`ui_widget`) |
|---|---|---|
| `draw` signature | `(&mut self, canvas, &UIState, &FontBook, &ColorTheme) -> Redraw` | `(&mut self, canvas, &ColorTheme, &FontBook) -> bool` |
| draw return meaning | `Redraw::{None,Now,Animating}` | `bool` = "still animating, schedule another frame" |
| pointer/keyboard | `on_cursor(&PointerEvent) -> Redraw` only | `on_pointer` / `on_keyboard`, both `-> bool` (consumed) |
| positioning | components position themselves ad hoc inside `draw` | host assigns via `set_layout_rect`; widgets must not own position independently |
| who constructs it | `UserInterface::new` in the bar | `m3_test`'s gallery / any future bar integration |
| currently used by the bar? | yes, exclusively | **no** |

Contract for `Widget::draw`: return `true` when an animation is still
traveling, so the host knows to schedule another frame (this is the `Widget`
equivalent of `Component`'s `Redraw::Animating`, just collapsed to a bool
since there's no `Now`/`None` distinction to make at this layer).
`on_pointer`/`on_keyboard` return `true` when the event was consumed.

### Layout containers (`linear.rs`, `scrollable.rs`)

- `Row(Linear)` / `Column(Linear)` — thin public newtypes over a private
  `Linear { axis, layout, children: Vec<Box<dyn Widget>>, gap, cross_align }`.
  `CrossAlign { Start, Center, End, Stretch }`. `push(child)`, `children()`,
  `children_mut()`, `child_mut(i)`. Main-axis size comes from summing
  children's `measure()` + `gap`; cross-axis follows `CrossAlign`.
- `ScrollableWidget { layout, children, content_height, offset, content_overlay, last_scroll }` —
  `new`, `push`, `set_content_height`, `set_content_overlay`, `offset`,
  `children`/`children_mut`/`child_mut`, `content_point` (viewport→content
  coordinate transform), `clamp_offset`. Constants: `SCROLL_LINE = 48.0`,
  `BAR_WIDTH = 4.0`, `BAR_MARGIN = 2.0`, `MIN_THUMB = 24.0`,
  `BAR_HOLD = 0.8s`, `BAR_FADE = 0.3s`, `BAR_MAX_ALPHA = 0.5`.

## `m3_widget` — Material 3 widget implementations

Modules: `animation`, `buttons/`, `checkbox`, `divider`, `drawing`, `icon`,
`list_item`, `slider`, `switch`, `text_field`, `tokens`. `lib.rs` re-exports
the widgets plus `ui_widget`'s `Row`/`Column`/`ScrollableWidget`/`Widget`, so
`use m3_widget::{Widget, Row, CheckBox, ...}` is enough for most consumers.

### Shared building blocks

- **`tokens`** — state-layer opacities `HOVER_OPACITY = 0.08`,
  `FOCUS_OPACITY = 0.10`, `PRESSED_OPACITY = 0.10`; disabled treatment
  `DISABLED_CONTAINER_OPACITY = 0.12`, `DISABLED_CONTENT_OPACITY = 0.38`.
  Individual widgets may deviate locally (e.g. `text_field` uses a 0.04
  disabled container opacity) rather than overriding the shared constant.
- **`animation`** — MD3 motion tokens, built on `ui_core::animation::parser`.
  `duration::{SHORT1..4 = 50/100/150/200, MEDIUM1..4 = 250/300/350/400,
  LONG1..4 = 450/500/550/600, EXTRA_LONG1..4 = 700/800/900/1000}` ms.
  `easing::{standard, standard_decelerate, standard_accelerate, emphasized,
  emphasized_decelerate, emphasized_accelerate}` — each is a function
  returning a *fresh* `Easing` (call once per `Animation` you construct, not
  shared). `standard() == cubic_bezier(0.2, 0.0, 0.0, 1.0)`. Convention:
  micro-interactions (hover/press/toggle) use `SHORT4` + `standard()`; large
  transitions use `MEDIUM1`/`LONG1` + `emphasized()`.
- **`drawing`** — `fill_circle(canvas, center, radius, color)`,
  `stroke_circle(canvas, center, radius, width, color)`; both no-op on a
  transparent color or non-positive radius, so callers don't need to guard.
- **`icon`** — `enum Icon { Check, Close, Add, Menu, Favorite, Settings, More, ArrowBack }`,
  `Icon::draw(self, canvas, box_rect, color)`, `BOX_SIZE = 24.0`,
  `ICON_STROKE_WIDTH = 2.0`. Paths built with Skia `PathBuilder`.

### Widgets (all implement `ui_widget::Widget`)

| Widget | Notable fields/API |
|---|---|
| `Divider` | `orientation, leading_inset, trailing_inset, thickness`. Builder: `.vertical().leading_inset(16.0).trailing_inset(4.0).thickness(2.0)`. Static — `draw` always returns `false`. |
| `CheckBox` | `checked, indeterminate, enabled, hovered, pressed, focused, on_change, animations.selection: Animation<f32>` (SHORT4 + standard). `SIZE = 40.0` (touch target), `BOX_SIZE = 18.0`. API: `CheckBox::new(checked).indeterminate(true).on_change(cb)`, `set_checked`, `checked()`, `indeterminate_value()`. `hit_rect()` outsets the 18px box to the 40px touch target. |
| `Switch` | `checked, icons: SwitchIcons{None,Selected,Both}, animations.{selection (MEDIUM1+emphasized), press (SHORT4+standard)}`. `TRACK_WIDTH = 52.0`, `TRACK_HEIGHT = 32.0`. API: `Switch::new(checked).icons(..).enabled(..).on_change(cb)`. |
| `Slider` | `value, min, max, step: Option<f32>, size: SliderSize, labeled, animations.{handle_width, indicator}`. `SliderSize { ExtraSmall, Small, Medium, Large, ExtraLarge }` each with `track_height()`/`handle_height()`/`corner_radius()`. API: `Slider::new(min, max, value).size(..).step(5.0).labeled(true).on_change(cb)`. |
| `TextField` | `text, cursor, font_key, enabled, hovered, focused, scroll_offset, on_change`. `HEIGHT = 56.0`. No animations. |
| `ListItem` | `headline, supporting, overline, trailing_text, leading/trailing: Option<IconDrawer>, on_click`, where `type IconDrawer = Box<dyn Fn(&Canvas, Rect, Color)>`. API: `ListItem::new("Headline").supporting(..).overline(..).trailing_text(..).leading(closure).on_click(cb)`. |
| `buttons::Button` | `ButtonVariant { Filled(default), FilledTonal, Elevated, Outlined, Text }`, `ButtonShape { Round(default), Square }`, `ButtonSize { ExtraSmall, Small(default), Medium, Large, ExtraLarge }` with `height()`/`horizontal_padding()`/`label_size()`/`corner_radius(pressed)`. `animations.shape` (SHORT4+standard) morphs round→squarer on press. API: `Button::new("Label").variant(..).shape(..).size(..).font(key).on_click(cb)`. |
| `buttons::IconButton` | `IconButtonVariant { Standard(default, no container), Filled, FilledTonal, Outlined }`. Has both `on_click` (non-toggle mode) and `on_change` (toggle mode) — mutually exclusive; `toggle(bool)`, `selected()`, `set_selected()`. Accepts either an `Icon` or a custom `icon_fn: Box<dyn Fn(&Canvas, Rect, Color)>`. |
| `buttons::RadioButton` | Same shape as `CheckBox` (`selected` + `on_change`), single-choice semantics left to the caller. |
| `buttons::SegmentedButton` | Newest widget (commit `b167598`). Deliberately **one `Widget` owning every segment** rather than a `Row` of children, because segments share one outline and internal dividers — splitting it into children would duplicate that geometry. `Segment { label: Option<String>, icon: Option<Icon>, enabled }` via `Segment::new(label)` / `Segment::icon_only(icon)`. `SelectionMode { Single(default), Multi }`. Fields: `segments, mode, selected: Vec<bool>, hovered/pressed: Option<usize>, focused_index, measured_widths: Option<Vec<f32>>` (cached per-segment widths from the last `draw`, reused for hit-testing), `on_change: Option<Box<dyn FnMut(usize, bool)>>`. |

### Recipe for a new `m3_widget` widget

Derived from the four most recently added widgets (`CheckBox`, `IconButton`,
`RadioButton`, `SegmentedButton`):

1. Struct = config (variant/size/label) + interaction state (value, enabled,
   hovered, pressed, focused) + `layout_rect: LayoutRect` + a nested
   `Animations` struct of `Animation<f32>` fields + callback (`on_change` /
   `on_click`, as `Option<Box<dyn FnMut(..)>>`).
2. Builder methods take `mut self` and return `Self` (for construction-time
   configuration); pair each with an imperative `set_*`/`clear_on_*` method
   for post-construction mutation.
3. `impl Widget`: `draw` paints container → state layer → content, ticks
   animations, returns `is_traveling()`; `on_pointer` updates
   hover/press from `PointerEventKind` and fires the callback on a matched
   press+release pair inside `hit_rect()`; `on_keyboard` handles
   Space/Enter (`KeyboardEventKind::Commit(" ")` and `Press{keysym}` both, per
   `CheckBox::on_keyboard` — different input backends route printable keys
   differently); `set_layout_rect`/`layout_rect` are plain accessors;
   `focusable()` returns `self.enabled`; `set_focused` sets the field.
4. Reuse `crate::tokens::*` opacities, `crate::drawing::fill_circle` for state
   layers, `crate::animation::{duration, easing}` for motion constants, and
   `crate::icon::Icon` where applicable — don't re-derive these per widget.
5. Fire callbacks only on genuine user interaction (a completed
   press-then-release, a committed key), never during construction or from a
   programmatic `set_*` call — see `CheckBox`'s `set_checked_does_not_report`
   test for the expected contract.

## `macros` — the `boxed!` macro

`macros/src/lib.rs` is exactly one `macro_rules!` macro (not a proc macro):

```rust
#[macro_export]
macro_rules! boxed {
    ($($e:expr),* $(,)?) => { vec![$(Box::new($e)),*] };
}
```

`boxed!(a, b, c)` expands to `vec![Box::new(a), Box::new(b), Box::new(c)]`.
Used by `src/ui.rs` to build the bar's component list and by `m3_test` to
build child-widget vectors.

## `m3_test` — the widget dev/demo loop

```sh
cargo run -p m3_test
```

A standalone winit + softbuffer + Skia gallery binary that exercises every
`m3_widget` widget — **this is the development loop for widget work**. It
does not need Wayland layer-shell or a running compositor bar; it's a normal
windowed application (`winit::application::ApplicationHandler`).

`main.rs` lays out each gallery section with `taffy::TaffyTree` and renders
with `skia_safe::surfaces::raster_n32_premul`. Constants: `ROOT_PADDING_X =
24.0`, `HEADER_HEIGHT = 80.0`, `SECTION_GAP = 32.0`. `util.rs` provides
`button_code()`, `load_theme()`, `resolve_layout_rects()`, `row_style()`,
`column_style()`, `item_style()`, `keysym_from_named()`, plus the `PendingWidget`
and `Section` types used to assemble each gallery page (one `*_gallery`
function per widget kind, e.g. `checkbox_gallery`, `segmented_gallery`).

When adding a new `m3_widget` widget, add a matching `*_gallery` function in
`m3_test/src/main.rs` so it's visually exercisable — this is how every
existing widget was iterated on.
