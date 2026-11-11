# Adding a component

A component is a function in `src/components/` that takes `&State` and returns a `Part`
(`src/ui.rs`). `clock.rs` is the smallest; `media.rs` has an island and an overlay;
`toolbox/` has two panels in one island.

## 1. A part in the bar

```rust
use anti_webui::prelude::*;

use crate::{state::State, ui::{palette, BarSlot, Part}};

pub fn my_part(state: &State) -> Part {
    let state = state.clone();
    let view = text(move || state.focused_title.get().unwrap_or_default())
        .whitespace_nowrap()
        .dyn_style(move |s| s.text_color(palette(&state, |t| t.on_surface)));
    Part { bar: Some((BarSlot::Left, view.into_view())), island: None, overlay: None }
}
```

Register it in `src/components/mod.rs` and add it to the `parts` list in `main.rs`. The order
there is the order within each `BarSlot`.

- Read signals inside closures (`text(|| ..)`, `dyn_style`, `dyn_view`, `effect`) so the view
  follows them. A `get()` outside one reads once.
- `palette(state, |t| t.role)` reads a theme color reactively. md3e widgets get the same
  scheme through `set_color_scheme`, set in `main.rs` from `theme.toml`.
- `Right` parts run to the screen's edge; add your own padding.
- Open and close on `on_pointer_down`, not `on_click`, which waits for release. Check
  `e.button` for `PointerButton::Primary`/`Secondary`.

## 2. An island

```rust
let open = signal(false);
let island = IslandPart {
    request: memo(move || open.get().then_some(IslandRequest {
        width: 360.0,
        height: 120.0,
        placement: IslandPlacement::Joined { center_x: None },
        priority: 100,
    })),
    content: my_card(&state).into_view(),
    close: Rc::new(move || open.set(false)),
    frame: signal(None),
};
```

- Return a request for as long as you want the island, and `None` to collapse it. Changing
  the size morphs it. `height` is clamped to `ISLAND_MAX_HEIGHT`.
- The highest `priority` wins the main island, which takes the pointer; on a tie, the later
  part does. The toolbox uses 150. A losing island collapses where it is and opens again when
  it next wins.
- The content is laid out at the request's full size, and scaled and faded while the island
  morphs, so text never reflows mid-animation. It stays shown while collapsing, after the
  request is gone, so keep what you were showing.
- `close` is called for a press outside the bar and the island, and for a press on another
  part of the bar.
- Placements: `Joined` rises from a point on the bar. `OverBar` also reaches down over the
  bar's row. `Corner { from_width }` is flush with the right edge, widening from
  `from_width`; the toolbox uses it.
- The morph's easing is `island` in `animation.toml`.

### Moving things between the bar and the island

`bar` writes `IslandFrame { progress, content }` to `frame` every frame the island shows:
how far open it is, and where the content sits when fully open, both from the top-left of
the part's place in the bar. An `overlay` view, drawn above the bar and every island from
that same origin, can read it to move something across. The toolbox's icons fly to their
entries; the media part's title, text lines and art travel to the card. Hide the bar's own
copy while the frame is `Some`.

### Text

Size the request from the text: `ui::measure(size)` gives a width function, and
`ui_core::util::wrap_text`/`elide` fit text to a width. The bar sets the font family (Noto
Sans CJK JP) on its root, so views inherit it.

### Timed animation

`ui::eased(state, easing_id, duration, target)` returns a signal that follows `target` with
the named easing from `animation.toml`, stepping with `request_frame`. AntiWebUI's
`animated(0.0)` with `.to(value, Transition)` works too where no configurable easing is
needed.

## 3. Adding to the toolbox

`items(state)` in `src/components/toolbox/items.rs` returns the toolbox's `Item`s. Their
order is the order in the bar and, within each `DetailWidth`, in the panel. An `Item` has a
size, a `bar_icon` (or none), where its icon lands in the panel (`icon_at`), whether it fades
instead (`icon_fades`), and its view. Build one with `tile` (icon, title, one line of detail,
something trailing) or `slider_row` (icon, slider, percentage) and look at the existing
entries for the shape.

- Values the user changes show what was asked at once: `optimistic` for choices (WARP, the
  power profile) shows the click's value until the status changes or 3s pass, and sliders
  ignore incoming values for `SLIDER_LEAD` after a move.
- Use md3e components (`styled_slider`, `styled_switch`, `connected_button_group`) and
  `icons::*` for glyphs.
- Polled values live in `SystemStatus` (`src/system.rs`); add a field and read it in the 5s
  poll, and a `SystemCommand` if it can be changed. Prefer an AntiWebUI shell provider when
  one reports changes as they happen, as the volume and brightness do.

The notifications panel (`toolbox/notifications.rs`) stacks each app's notifications into
one row. A row goes by a sideways mouse drag past 40% of its width, or its ✕. Touchpad
sideways scrolling deliberately does not dismiss: it misfired too often.

## 4. Shared state

`State` (`src/state.rs`) is `Clone`; every part gets the same signals. Write to them only
through `State`'s methods (`dismiss_notification`, `clear_notifications`) or a provider's
channel; send commands through `state.commands`.

| Signal | Updated by |
|--------|------------|
| `players` | MPRIS events, and polled positions when they drift |
| `player_art` | the cover-art watcher |
| `last_playing` | MPRIS: the player that played last |
| `warp` | WARP |
| `notifications` | the notification server, the expiry tick, dismissals |
| `system` | the 5s poll; volume from PipeWire, brightness from the backlight |
| `focused_title`, `focused_workspace` | compositor IPC |
| `theme`, `animation` | config reloads |
| `action` | `my_shell action` |
