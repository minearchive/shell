# Adding a Component

Components are the primary UI extension point **for the bar binary**. Each
lives in `src/components/`. `src/components/clock.rs` is the minimal reference
(state-only, no animation); `src/components/notifications.rs` shows an island.

`Component` is unrelated to `ui_widget::Widget` — see
[docs/widgets.md](widgets.md) if you were looking for the M3 widget stack
instead. This doc is about `src/ui.rs`'s `Component` trait, used only by the
bar's own drawing.

## 1. Implement the trait

The exact current signature (`src/ui.rs`):

```rust
pub trait Component {
    fn draw(
        &mut self,
        canvas: &Canvas,
        state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> Redraw;
    fn on_cursor(&mut self, _events: &PointerEvent) -> Redraw { Redraw::None }
    fn on_ipc(&mut self, _events: &IPCEvent) -> Redraw { Redraw::None }
    fn on_mpris(&mut self, _state: &PlayerState, _event: &MprisEvent) -> Redraw { Redraw::None }
    fn on_kde_connect_event(&mut self, _event: &KDEConnectEvent) -> Redraw { Redraw::None }
    fn on_warp(&mut self, _status: &WarpStatus) -> Redraw { Redraw::None }
    fn on_action(&mut self, _action: &Action) -> Redraw { Redraw::None }
    fn on_notification(&mut self, _event: &NotificationEvent) -> Redraw { Redraw::None }
    fn on_easing_updated(&mut self, _id: &str, _easing: &Easing) -> Redraw { Redraw::None }
}
```

`draw` takes `&mut self` (components mutate their own state while drawing —
e.g. a hit-test rect) and returns `Redraw`
(`None` / `Now` / `Animating`), not `()`. `theme: &ColorTheme` is the resolved
active-mode M3 palette (see [docs/config.md](config.md)); `fonts: &FontBook`
comes from `ui_core::font`.

```rust
// src/components/my_widget.rs
use crate::ui::{Component, Redraw, UIState};
use crate::ipc::events::IPCEvent;
use crate::dbus::mpris::PlayerState;
use mpris::Event as MprisEvent;
use skia_safe::Canvas;
use smithay_client_toolkit::seat::pointer::PointerEvent;
use ui_core::{font::FontBook, scheme::ColorTheme};

pub struct MyWidget { /* fields */ }

impl Component for MyWidget {
    fn draw(&mut self, canvas: &Canvas, state: &UIState, fonts: &FontBook, theme: &ColorTheme) -> Redraw {
        /* … */
        Redraw::None
    }

    // Everything has a default body in the trait — implement only what your
    // component needs. The full set beyond draw: bar_slot, bar_width, island,
    // draw_island, on_island_cursor, on_cursor, on_ipc, on_mpris,
    // on_kde_connect_event, on_warp, on_notification, on_easing_updated,
    // on_drag_enter, on_drag_motion, on_drag_leave, on_drop
    fn on_mpris(&mut self, _: &PlayerState, _: &MprisEvent) -> Redraw { Redraw::None }
}
```

Only `draw` is required; every other trait method has a default `Redraw::None`
body, so you only override the ones your component reacts to.
**Never leave `todo!()` in any `on_*`** — these are called unconditionally for
every event and will crash the process.

Return `Redraw::Now` when something changed and needs one repaint, and
`Redraw::Animating` while an `Animation` is still traveling (see
`UserInterface::draw` does for an island's morph) —
`Animating` is what keeps a running animation's frames coming: `Shell::draw`
leaves that screen's `needs_redraw` set, so its next frame callback repaints it.

## 2. Register the module and instantiate

`src/components/mod.rs`:
```rust
pub mod my_widget;
```

`src/ui.rs` → `UserInterface::new`, via the `boxed!` macro (`macros::boxed!`,
a `macro_rules!` that expands `boxed!(a, b)` to `vec![Box::new(a), Box::new(b)]`):
```rust
let components: Vec<Box<dyn Component>> = boxed!(
    Clock,
    Media::new(player_commands),
    Toolbox::new(system_commands, warp_commands),
    MyWidget::new(/* … */),
);
```
`UserInterface::new` itself takes `(theme, animation, ui_tx, system_commands, warp_commands, player_commands)` — pass
whichever your new component needs through to its constructor from there.

Bar placement comes from `bar_slot` (`Left`, `Right`, or the default `None`) and
`bar_width`. `Right` slots run to the screen's edge, so add your own padding. `UserInterface`
packs each end and translates the canvas, so `draw`'s
origin is your slot's top-left corner, and `on_cursor` positions are relative to it.
`ui::bar_baseline(&font)` centers text vertically.

## 3. Growing an island

`Media` (`src/components/media.rs`) and `Toolbox` are the references. A bar has one
island, so components bid for it:

```rust
fn island(&mut self, state: &UIState, fonts: &FontBook, max_width: f32) -> Option<IslandRequest> {
    state.my_thing.as_ref()?;
    Some(IslandRequest {
        width: 300.0,
        height: 80.0,
        placement: IslandPlacement::Joined { center_x: None },
        priority: 100,
    })
}

fn draw_island(&mut self, canvas: &Canvas, size: (f32, f32), state: &UIState,
               fonts: &FontBook, theme: &ColorTheme) -> Redraw {
    // Origin is the content's top-left corner at full size.
    Redraw::None
}

fn on_island_cursor(&mut self, event: &PointerEvent) -> Redraw {
    // `event.position` is relative to the content's origin.
    Redraw::None
}
```

- `island` runs before every draw. Keep returning a request while you want the
  island, and return `None` to collapse it. Changing the size morphs it.
- The highest `priority` wins the main island, the only one that stays open, and takes
  the pointer. On a tie, the later component does. The toolbox uses 150. Every component has its own island, so one that loses collapses where it is
  while the winner opens where it is, and opens again when it next wins.
- `draw_island` keeps being called while the island collapses, after `island` has
  returned `None`, so keep a copy of what you were showing.
- Draw at full size. The island scales and fades your content while it morphs, so
  text never reflows mid-animation.
- Size your request from the text with `ui_core::util::wrap_text` and draw it with
  `FontBook::draw`, which falls back for glyphs the font lacks.
- `height` is clamped to `ISLAND_MAX_HEIGHT` (`src/ui.rs`).
- Islands rise from the bar's top edge in the bar's color, joined to it by curves.
  `IslandPlacement::Joined` sits over a point on the bar and scales from nothing at its
  bottom center. `IslandPlacement::OverBar` does the same but reaches down over the bar to
  the screen's edge, so presses on that part of the bar's row go to the island.
- Notifications are not an island. The one on display drops in as a card from the top of
  the screen, on a surface of its own (`src/banner.rs`, drawn by `Banner` in
  `src/components/notifications.rs`), and a click dismisses it.
  `IslandPlacement::Corner { from_width }` is flush with the screen's right
  edge, widening from `from_width`, with content scaling from the bottom-right.
- `draw_overlay` draws over the island's content, unscaled, at your bar slot's origin, and
  clipped to the bar and islands. While your island is visible it receives an `IslandFrame`
  with `progress` and the content's final rect, so something can move between the bar and
  the island. The toolbox's icons and the media player's icon and text do this.
- To dismiss on a press outside the bar, return `true` from `captures_outside_press` while
  open and handle `on_outside_press`. `src/catcher.rs` puts a transparent surface over that
  screen meanwhile, so the press doesn't reach the window under it.
- A press inside the island keeps delivering pointer events to its owner until release,
  so a slider drag can leave the island.
- Every screen runs its own island from the shared `UIState`. A component that changes
  shared state from a click sends a `UiEvent`, as `DismissNotification` does.

The morph's easing comes from the `island` key in `animation.toml`.

## 4. Adding to the toolbox

The toolbox (`src/components/toolbox/`) shows a row of icons in the bar's bottom-right
corner. Clicking it opens a panel of details above it, as a `Corner` island, and each icon
flies to its entry in the panel. Clicking the row again, the rest of the bar, or anywhere
else on that screen closes it. A right click opens the unseen notifications in the same
corner instead (`notifications.rs`), laid out like Android's shade and always three cards
tall, scrolling past that. Each app's notifications stack into one row that a click opens.
A row goes by dragging it sideways, a sideways touchpad swipe, or its ✕ on hover. The
icons fade where they are, and a badge on the row counts the unseen ones while it is
closed. `Caption` entries go across the top of the panel,
half-width entries pair up in rows below them, and full-width entries stack below those. To add one, push it to
`default_items` in `items.rs`; its position there sets its order in the bar and panel:

```rust
// A read-only tile. The closure returns its icon and line of detail.
Box::new(InfoItem::new(DetailWidth::Half, "VPN", |status| {
    (Icon::Check, status.vpn_name.clone())
})),

// A slider. Arguments: minimum, icon, current value (None disables it), on change.
Box::new(SliderItem::new(
    0.0,
    |_| Icon::Brightness,
    |status| status.mic_level,
    move |value| { let _ = tx.send(SystemCommand::MicLevel(value)); },
)),
```

`CaptionItem` is a small right-aligned line that the bar's icon fades into, as the battery
does, and takes no space while it has nothing to say. `ChoiceItem` picks one of several options with a `SegmentedButton`; see the power profile
entry. `SliderItem::panel_only()` leaves an entry's icon out of the bar. Anything else
implements `ToolboxItem` directly. Add new glyphs to `m3_widget::Icon`.

`SystemStatus` and its polling live in `src/system.rs`. Add a field, read it in
`read_fast` (what the user adjusts, 1s) or `read_slow` (5s), and add a `SystemCommand`
if it can be changed.

## 5. Triggering redraws

Components own no threads. Data arrives through a provider channel (see
[docs/data-sources.md](data-sources.md)) and the `on_*` handler's `Redraw` return
value requests the repaint. For time-driven content, read the clock in `draw` the
way `Clock` does: the `CLOCK_TICK` timer in `main()` calls
`shell.request_redraw_all()` every second.

`shell.request_redraw(idx)` sets `Screen.needs_redraw` and draws immediately
unless a `wl_surface.frame` callback is still outstanding. There is no separate
dirty flag on `UIState`. See CLAUDE.md's "Rendering is event-driven" section for
the full model.

## 6. Using fonts

`FontBook` (`ui_core/src/font/mod.rs`) maps short string keys to `Typeface`s.
`fonts.sized("noto_sans", 32.)` returns a 32px `skia_safe::Font` and is always
available: `noto_sans` (Noto Sans CJK JP) is the only key registered by default,
in `main()` right after `FontBook::new()`.

To use another font, add a call next to it:

```rust
book.register("roboto", "Roboto", FontStyle::normal());
```

`register` panics if Skia's `FontMgr` cannot resolve the family, and **calling
`fonts.sized` with an unregistered key panics**: there is no silent fallback.
Register the key before any `draw` call can reach it.

## 7. Reading shared state

`UIState` (`src/ui.rs`) is the render snapshot passed by reference to every
`draw`. `Shell` owns the single copy shared by every screen. Components **read**
it and never write to it; `main()`'s channel closures update it just before
dispatching the event (e.g. `state.update_player`).

| Field | Type | Updated by | Meaning |
|-------|------|------------|---------|
| `players` | `HashMap<String, PlayerState>` | MPRIS channel, position polling | Active MPRIS players, keyed by bus name; inactive ones are removed |
| `player_art` | `HashMap<String, Art>` | Player-art channel | Cover art, keyed by bus name like `players` |
| `warp` | `Option<WarpStatus>` | WARP channel | Latest Cloudflare WARP status, if connected |
| `notifications` | `NotificationStore` | Notification channel, clock tick, `DismissNotification`, `ClearNotifications` | Notifications on display; `current()` is the one to show. The tick moves expired ones to a missed list; `unseen()` returns both, newest first, for the toolbox |
| `system` | `SystemStatus` | System channel | Network, Bluetooth, volume, backlight, battery and power profile |

To surface new data to components:

1. Add the public field to `UIState`, with a `Default` (`UIState` derives it).
2. Update it in the provider's channel closure in `main()`, before
   `shell.dispatch`, so the repaint that dispatch may trigger sees it. The
   repaint itself is driven by the handler's `Redraw` return value; see
   [docs/data-sources.md](data-sources.md).
