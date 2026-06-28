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
    fn on_cursor(&self, _: &PointerEvent) {}
    fn on_ipc(&mut self, _: &IPCEvent) {}
    fn on_mpris(&mut self, _: &PlayerState, _: &MprisEvent) {}
}
```

All four methods must be implemented. Use empty bodies for unused ones.
**Never leave `todo!()` in `on_mpris`** — it is called unconditionally for every
player event and will crash the process.

## 2. Register the module and instantiate

`src/components/mod.rs`:
```rust
pub mod my_widget;
```

`src/ui.rs` → `UserInterface::new` (around line 72):
```rust
let components: Vec<Box<dyn Component>> = vec![
    Box::new(Clock::new(rx.clone(), idx, 1000, Arc::clone(&config))),
    Box::new(MyWidget::new(/* … */)),
];
```

## 3. Triggering redraws from a background thread

Hold a clone of the `Sender<UiEvent>` and the screen index passed into
`UserInterface::new`. Send from any thread:

```rust
let _ = sender.send(UiEvent::RequestRedraw(screen_idx));
```

`UiEvent::RequestRedraw(idx)` is handled in `main.rs:141` by calling
`Shell::draw(idx)` directly. There is no separate dirty flag.

## 4. Using fonts

`fonts.sized("noto_sans", 32.)` is always available — it is the only font
registered by default (`main.rs:184`, Noto Sans CJK JP).

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
| `workspace_id` | `String` | Current focused workspace |
| `window_title` | `String` | Title of the focused window |
| `players` | `HashMap<String, PlayerState>` | Active MPRIS players |
| `padding` | `f32` | Horizontal scroll offset (debug feature) |

To surface new data to components, add a field to `UIState` and update it in the
relevant `UserInterface::on_*` handler. See [docs/config.md](config.md) for
details on `UIState` and `Configuration`.
