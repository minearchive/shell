# Adding a component

A component is a function in `src/components/` that takes `&State` and returns a `Part`
(`src/ui.rs`). `clock.rs` is the smallest; `media.rs` has an island and an overlay;
`toolbox/` has two panels in one island; `osd.rs` is an island with no place in the bar.

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
        passive: false,
    })),
    content: my_card(&state).into_view(),
    close: Rc::new(move || open.set(false)),
    frame: signal(None),
};
```

- Return a request for as long as you want the island, and `None` to collapse it. Changing
  the size morphs it. `height` is clamped to `ISLAND_MAX_HEIGHT` (480).
- The highest `priority` wins the main island, which takes the pointer; on a tie, the later
  part does. The priorities in use: toolbox 150, tray 140, media 50, OSD 20. A losing island
  collapses where it is and opens again when it next wins.
- The content is laid out at the request's full size, and scaled and faded while the island
  morphs, so text never reflows mid-animation. It stays shown while collapsing, after the
  request is gone, so keep what you were showing.
- `close` is called for a press outside the bar and the island, and for a press on another
  part of the bar.
- Placements: `Joined` rises from a point on the bar. `OverBar` also reaches down over the
  bar's row. `Corner { from_width }` is flush with the right edge, widening from
  `from_width`; the toolbox uses it.
- The morph's easing is `island` in `animation.toml`.

### Islands that only show something (`passive`)

Set `passive: true` for an island the user did not open and that should not take the
pointer, like the OSD (`components/osd.rs`):

- It takes no input, and the outside-press catcher does not grow to cover the screen, so a
  press while it shows reaches whatever is under it, as if the island were not there.
- It does not count as open: `Bar::open` stays `false` while only passive islands show, so
  the catcher stays shrunk to a pixel.
- It still bids like any island and loses to a higher `priority`. The OSD sits below media
  and the toolbox, so it does not appear while either is open.

A passive island still needs a `close` (the bar calls it like any other) and still gets a
`frame`; `close` is how a timed island hides itself, as the OSD's `set_timeout` does.

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

`ui::Eased::new(state, easing_id, duration, start)` is the object form: `.to(v)` glides to
`v` from where it is, `.set(v)` jumps there at once, and `get()` reads it reactively. It
follows `state.animation` in an effect, so a reload swaps the curve mid-motion. The
toolbox's volume and brightness sliders and the OSD use it with `slider_level`: a value
that changed elsewhere glides over 250ms, while a drag and the first value jump.

### A timed island (the OSD)

`components/osd.rs` shows the volume or brightness for 1.5s after either changes:

- It watches `state.system` in an effect and compares against the last values, so the first
  value at startup and a device appearing are not treated as changes.
- `set_timeout` inside an effect restarts the countdown each time the effect reruns; the
  timer belongs to the run that set it. The OSD bumps a counter signal to rerun it.
- It keeps the last shown icon and level in a signal, because the content stays visible
  while the island collapses after the request is gone.
- A change from a key or another app is not the slider's drag, so the level glides with
  `Eased` rather than jumping.

## 3. Adding to the toolbox

`items(state)` in `src/components/toolbox/items.rs` returns the toolbox's `Item`s. Their
order is the order in the bar and, within each `DetailWidth`, in the panel. An `Item` has a
size, a `bar_icon` (or none), where its icon lands in the panel (`icon_at`), whether it fades
instead (`icon_fades`), and its view. Build one with `tile` (icon, title, one line of detail,
something trailing) or `slider_row` (icon, slider, percentage) and look at the existing
entries for the shape.

- `bar_icon` is a `BarIcon` (`Rc<dyn Fn(&SystemStatus) -> Option<&'static Symbol>>`), not a
  plain `fn`, so an entry can depend on a signal outside `SystemStatus`: do not disturb shows
  `NOTIFICATIONS_OFF` in the bar while `state.quiet` is on, and fades onto the tile's own icon
  as the panel opens.
- Values the user changes show what was asked at once: `optimistic` for choices (WARP, the
  power profile) shows the click's value until the status changes or 3s pass, and sliders
  ignore incoming values for `SLIDER_LEAD` (600ms) after a move.
- Sliders glide to a value that changed elsewhere with `Eased` and `slider_level`, and jump
  on the first value and while dragging.
- Use md3e components (`styled_slider`, `styled_switch`, `connected_button_group`) and
  `icons::*` for glyphs.
- Polled values live in `SystemStatus` (`src/system.rs`); add a field and read it in the 5s
  poll, and a `SystemCommand` if it can be changed. Prefer an AntiWebUI shell provider when
  one reports changes as they happen, as the volume and brightness do.

### The toggles

After the Wi-Fi and Bluetooth tiles, two to a row: do not disturb (おやすみ), keep awake
(画面オン), night light (ナイトライト) and mic (マイク). `toggle(state, label, symbol, on,
set)` draws one; `on` returns `None` when the thing can't be used, and the tile dims and
ignores presses.

| Toggle | Signal | What it does |
|--------|--------|--------------|
| おやすみ | `state.quiet` | Non-critical notifications skip the banner and go straight to the missed ones on the next tick. Critical ones still show. Remembered across restarts. |
| 画面オン | `state.awake` | Holds logind's idle inhibitor (`toggles::inhibit_idle`); hypridle and swayidle respect it. |
| ナイトライト | `state.night_light` | Runs `gammastep -O 4000` as a child process (`toggles::night_light`). |
| マイク | AntiWebUI `shell-audio` | Mutes the default source; not held in `State`, so it reads the real state. |

The side effects are held in `src/toggles.rs` and start from effects in `State::start`, not
from the views: the inhibitor is an `OwnedFd` that is dropped (released) when the signal goes
off, and the night light's process is sent `SIGTERM` so it restores the screen's colours. Both
end with the bar: the descriptor closes with the process, and the child gets `SIGTERM` from
`PR_SET_PDEATHSIG` if the bar crashes.

If one can't be started (no logind, no `gammastep`), the effect logs a warning and turns its
signal back off, so the tile never shows "オン" for something that isn't held.

Do not disturb is remembered in `$XDG_STATE_HOME/my_shell/quiet` (see
[docs/config.md](config.md#remembered-state)); the other toggles are not.

The notifications panel (`toolbox/notifications.rs`) stacks each app's notifications into
one row. A row goes by a sideways mouse drag past 40% of its width, or its ✕. Touchpad
sideways scrolling deliberately does not dismiss: it misfired too often.

## 4. The system tray

`src/components/tray.rs` draws the icons of programs that register a StatusNotifierItem,
in the bar left of the toolbox, and opens the clicked one's menu as its island.

- The row is `for_each(move || tray().items(), |item| item.key(), ..)`: the key is the
  item's bus name and path, so a view stays with its item as the list changes. The row
  hides itself (and takes no room) while there are no items.
- An icon is `bitmap_view(move || Some(item.icon.best(size)))`; an item with no picture
  falls back to the first letter of its title.
- Clicks follow the item: primary `activate` (or open the menu when `item_is_menu`),
  secondary `context_menu` when it has a dbusmenu, middle `secondary_activate`, and the
  wheel goes to `scroll`. Items without a dbusmenu get their own menu from the program.
- The menu is flattened with `flatten` (`TrayMenuItem` → `Row`): hidden entries dropped,
  submenus indented in place, `Check`/`Radio` that are on shown with a ✓, separators kept.
  An island cannot open a second menu, so submenus are not nested popups.
- The island's height is the row count, capped at `MENU_MAX_ROWS`: the island does not
  scroll, so a longer menu is cut off.
- Priority 140: below the toolbox's panel (the user opened that on purpose) and above the
  OSD.

The tray is read straight from AntiWebUI's provider rather than copied into `State`; see
[docs/data-sources.md](data-sources.md#providers-read-directly).

## 5. Shared state

`State` (`src/state.rs`) is `Clone`; every part gets the same signals. Write to them only
through `State`'s methods (`dismiss_notification`, `clear_notifications`) or a provider's
channel; send commands through `state.commands`.

| Signal | Updated by |
|--------|------------|
| `players` | MPRIS events, and polled positions when they drift |
| `player_art` | the cover-art watcher |
| `last_playing` | MPRIS: the player that played last |
| `warp` | WARP |
| `notifications` | the notification server, the expiry tick, dismissals, the low-battery warning |
| `system` | the 5s poll; volume from PipeWire, brightness from the backlight |
| `focused_title`, `focused_workspace` | compositor IPC |
| `theme`, `animation` | config reloads |
| `action` | `my_shell action` |
| `quiet`, `awake`, `night_light` | the toolbox's toggles; `State::start` holds the inhibitor and `gammastep` while on (`src/toggles.rs`) |

### The low-battery warning

`State::warn_on_low_battery` (`src/state.rs`) posts a notification through the same store the
D-Bus server fills, keyed by `LOW_BATTERY_ID` (`u32::MAX`, which the server never hands out):

- It fires once per mark in `system::LOW_BATTERY` (`[20, 5]`, percent) while the battery
  discharges, and takes the notification back when it charges past them.
- The last mark is `Urgency::Critical`, so it shows through do not disturb; the first is
  `Normal`.
- `Battery::low()` (`src/system.rs`) counts the marks passed; the effect compares it against
  the last count, so a battery hovering at a mark does not re-notify.

