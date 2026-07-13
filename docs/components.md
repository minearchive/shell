# Adding a Component

Components are the primary UI extension point. Each lives in `src/components/`.
`src/components/clock.rs` is the canonical reference implementation.

## 1. Implement the trait

```rust
// src/components/my_widget.rs
use crate::ui::{Component, UIState, UiEvent};
use crate::ipc::events::IPCEvent;
use crate::dbus::mpris::PlayerState;
use crate::font::FontBook;
use mpris::Event as MprisEvent;
use skia_safe::Canvas;
use smithay_client_toolkit::seat::pointer::PointerEvent;

pub struct MyWidget { /* fields */ }

impl Component for MyWidget {
    fn draw(&self, canvas: &Canvas, state: &UIState, fonts: &FontBook) { /* … */ }

    // The rest have default no-op bodies in the trait — implement only what you
    // care about. on_mpris is shown as an example; the full set is:
    //   on_cursor, on_ipc, on_mpris, on_kde_connect_event,
    //   on_warp, on_notification, on_easing_updated
    fn on_mpris(&mut self, _: &PlayerState, _: &MprisEvent) {}
}

Only `draw` is required; every other trait method has a default empty body, so
you only override the ones your component reacts to.
**Never leave `todo!()` in any `on_*`** — these are called unconditionally for
every event and will crash the process.

## 2. Register the module and instantiate

`src/components/mod.rs`:
```rust
pub mod my_widget;
```

`src/ui.rs` → `UserInterface::new` (around line 67):
```rust
let components: Vec<Box<dyn Component>> = vec![
    Box::new(Clock::new(
        rx.clone(),
        idx,
        1000,
        Arc::clone(&config),
        Arc::clone(&animation),
    )),
    Box::new(MyWidget::new(/* … */)),
];
```

## 3. Triggering redraws from a background thread

Hold a clone of the `Sender<UiEvent>` and the screen index passed into
`UserInterface::new`. Send from any thread:

```rust
let _ = sender.send(UiEvent::RequestRedraw(screen_idx));
```

`UiEvent::RequestRedraw(idx)` is handled in `main.rs:202` by calling
`shell.request_redraw(idx)`. There is no separate dirty flag.

## 4. Using fonts

`fonts.sized("noto_sans", 32.)` is always available — it is the only font
registered by default (`main.rs:259`, Noto Sans CJK JP).

To use another font, send `UiEvent::RegisterFont` once before the first draw:

```rust
let _ = sender.send(UiEvent::RegisterFont(
    "roboto".into(),
    "Roboto".into(),
    skia_safe::FontStyle::normal(),
));
```

Calling `fonts.sized` with an unregistered key returns a silent fallback — no
panic, but wrong rendering.

## 5. Reading shared state

`UIState` (passed to `draw`) exposes:

| Field | Type | Meaning |
|-------|------|---------|
| `players` | `HashMap<String, PlayerState>` | Active MPRIS players, keyed by identity |
| `warp` | `Option<WarpStatus>` | Latest Cloudflare WARP status, if connected |

To surface new data to components, add a field to `UIState` and update it in the
relevant `UserInterface::on_*` handler. See [docs/data-sources.md](data-sources.md)
and [docs/config.md](config.md) for
details on `UIState` and `Configuration`.
