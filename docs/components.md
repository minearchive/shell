# Adding a Component

Components are the primary UI extension point **for the bar binary**. Each
lives in `src/components/`. `src/components/clock.rs` is the canonical
reference implementation (animated, reacts to pointer input and easing
reloads); `src/components/warp.rs` is the minimal reference (state-only,
no animation).

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
    fn on_notification(&mut self, _event: &NotificationEvent) -> Redraw { Redraw::None }
    fn on_easing_updated(&mut self, _id: String, _easing: &Easing) -> Redraw { Redraw::None }
}
```

`draw` takes `&mut self` (components mutate their own state while drawing —
e.g. `Clock` updates its hit-test `BoundingBox` there) and returns `Redraw`
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

    // The rest have default Redraw::None bodies in the trait — implement only
    // the ones your component reacts to. The full set beyond draw:
    //   on_cursor, on_ipc, on_mpris, on_kde_connect_event,
    //   on_warp, on_notification, on_easing_updated
    fn on_mpris(&mut self, _: &PlayerState, _: &MprisEvent) -> Redraw { Redraw::None }
}
```

Only `draw` is required; every other trait method has a default `Redraw::None`
body, so you only override the ones your component reacts to.
**Never leave `todo!()` in any `on_*`** — these are called unconditionally for
every event and will crash the process.

Return `Redraw::Now` when something changed and needs one repaint, and
`Redraw::Animating` while an `Animation` is still traveling (see
`Clock::draw`, which returns `Animating` until `self.animation.is_done()`) —
`Animating` is what keeps a running animation's frames coming, since
`UserInterface::draw` sends `UiEvent::RequestRedrawAll` whenever any component
reports it.

## 2. Register the module and instantiate

`src/components/mod.rs`:
```rust
pub mod my_widget;
```

`src/ui.rs` → `UserInterface::new`, via the `boxed!` macro (`macros::boxed!`,
a `macro_rules!` that expands `boxed!(a, b)` to `vec![Box::new(a), Box::new(b)]`):
```rust
let components: Vec<Box<dyn Component>> = boxed!(
    Clock::new(rx.clone(), idx, 1000, Arc::clone(&animation)),
    Warp::new(commands.warp),
    MyWidget::new(/* … */),
);
```
`UserInterface::new` itself takes `(rx: Sender<UiEvent>, idx, _ipc: &mut WindowManagerIPC,
commands: Commands, theme: Arc<RwLock<Theme>>, animation: Arc<RwLock<AnimationConfig>>)` —
pass whichever of `theme`/`animation`/`commands` your new component needs
through to its constructor from there.

## 3. Triggering redraws from a background thread

Hold a clone of the `Sender<UiEvent>` and the screen index passed into
`UserInterface::new`. Send from any thread:

```rust
let _ = sender.send(UiEvent::RequestRedraw(screen_idx));
```

`UiEvent::RequestRedraw(idx)` is handled in `main()`'s `UiEvent` channel
closure by calling `shell.request_redraw(idx)`, which sets
`Screen.needs_redraw` and draws immediately unless a `wl_surface.frame`
callback is still outstanding. There is no separate dirty flag on `UIState`.
See CLAUDE.md's "Rendering is event-driven" section for the full model.

## 4. Using fonts

`fonts.sized("noto_sans", 32.)` is always available — it is the only font
registered by default, in `main()` right after `FontBook::new()` (Noto Sans
CJK JP).

To use another font, send `UiEvent::RegisterFont` once before the first draw:

```rust
let _ = sender.send(UiEvent::RegisterFont(
    "roboto".into(),
    "Roboto".into(),
    skia_safe::FontStyle::normal(),
));
```

**Calling `fonts.sized` with an unregistered key panics** (`ui_core/src/font/mod.rs`,
`FontBook::sized`) — there is no silent fallback. Register the key before any
`draw` call can reach it.

## 5. Reading shared state

`UIState` (passed to `draw`) exposes:

| Field | Type | Meaning |
|-------|------|---------|
| `players` | `HashMap<String, PlayerState>` | Active MPRIS players, keyed by identity |
| `warp` | `Option<WarpStatus>` | Latest Cloudflare WARP status, if connected |

To surface new data to components, add a field to `UIState` and update it in the
relevant `UserInterface::on_*` handler. See [docs/data-sources.md](data-sources.md)
and [docs/config.md](config.md) for details on `UIState` and `Configuration`/`Theme`.
