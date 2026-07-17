use std::num::NonZeroU32;
use std::rc::Rc;

use serde::Deserialize;
use skia_safe::{surfaces, Color4f, Contains, ImageInfo, Paint, Point};
use softbuffer::{Context, Surface};
use ui_core::font::FontBook;
use ui_core::keyboard::{self, KeyboardEvent, KeyboardEventKind};
use ui_core::pointer::{self, AxisScroll, PointerEvent, PointerEventKind};
use ui_core::scheme::ColorTheme;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use m3_widget::{Button, ButtonSize, ButtonVariant, Slider, SliderSize, TextField, Widget};

fn button_code(button: MouseButton) -> Option<u32> {
    match button {
        MouseButton::Left => Some(pointer::button::LEFT),
        MouseButton::Right => Some(pointer::button::RIGHT),
        MouseButton::Middle => Some(pointer::button::MIDDLE),
        _ => None,
    }
}

/// Editing keys only — everything else (letters, digits, ...) arrives as text
/// via `KeyEvent::text` and is routed through `Commit` instead.
fn keysym_from_named(key: &NamedKey) -> Option<u32> {
    match key {
        NamedKey::Backspace => Some(keyboard::key::BACKSPACE),
        NamedKey::Tab => Some(keyboard::key::TAB),
        NamedKey::Enter => Some(keyboard::key::RETURN),
        NamedKey::Escape => Some(keyboard::key::ESCAPE),
        NamedKey::Home => Some(keyboard::key::HOME),
        NamedKey::ArrowLeft => Some(keyboard::key::LEFT),
        NamedKey::ArrowUp => Some(keyboard::key::UP),
        NamedKey::ArrowRight => Some(keyboard::key::RIGHT),
        NamedKey::ArrowDown => Some(keyboard::key::DOWN),
        NamedKey::End => Some(keyboard::key::END),
        NamedKey::Delete => Some(keyboard::key::DELETE),
        _ => None,
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct ThemeFile {
    is_dark: bool,
    dark: ColorTheme,
    light: ColorTheme,
}

impl Default for ThemeFile {
    fn default() -> Self {
        Self {
            is_dark: true,
            dark: ColorTheme::default(),
            light: ColorTheme::default(),
        }
    }
}

fn load_theme(path: &str) -> ColorTheme {
    match std::fs::read_to_string(path) {
        Ok(contents) => match toml::from_str::<ThemeFile>(&contents) {
            Ok(tf) => {
                if tf.is_dark {
                    tf.dark
                } else {
                    tf.light
                }
            }
            Err(e) => {
                eprintln!("m3_test: failed to parse {path}: {e}; using default theme");
                ColorTheme::default()
            }
        },
        Err(e) => {
            eprintln!("m3_test: failed to read {path}: {e}; using default theme");
            ColorTheme::default()
        }
    }
}

// ---- winit app ----

struct App {
    theme: ColorTheme,
    fonts: FontBook,
    widgets: Vec<Box<dyn Widget>>,
    window: Option<Rc<Window>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    cursor: pointer::Point,
    modifiers: keyboard::Modifiers,
    focus: Option<usize>,
}

/// Top of each gallery row. The section captions are drawn just above these,
/// so keep the two in step.
const VARIANTS: f32 = 100.0;
const SIZES: f32 = 180.0;
const DISABLED: f32 = 280.0;
const SLIDERS: f32 = 380.0;
const SLIDER_SIZES: f32 = 450.0;
const TEXT_FIELDS: f32 = 620.0;

/// Baseline offset from a row's top to its caption.
const CAPTION_OFFSET: f32 = 8.0;

/// One button per variant, then one per size, then the disabled treatments.
fn button_gallery() -> Vec<Box<dyn Widget>> {
    let variants = [
        ("Filled", ButtonVariant::Filled, 24.0, 100.0),
        ("Tonal", ButtonVariant::FilledTonal, 136.0, 100.0),
        ("Elevated", ButtonVariant::Elevated, 248.0, 110.0),
        ("Outlined", ButtonVariant::Outlined, 370.0, 110.0),
        ("Text", ButtonVariant::Text, 492.0, 90.0),
    ];

    let sizes = [
        ("XS", ButtonSize::ExtraSmall, 24.0, 90.0),
        ("S", ButtonSize::Small, 126.0, 90.0),
        ("M", ButtonSize::Medium, 228.0, 100.0),
        ("L", ButtonSize::Large, 340.0, 110.0),
        ("XL", ButtonSize::ExtraLarge, 462.0, 120.0),
    ];

    let disabled = [
        ("Filled", ButtonVariant::Filled, 24.0, 100.0),
        ("Tonal", ButtonVariant::FilledTonal, 136.0, 100.0),
        ("Outlined", ButtonVariant::Outlined, 248.0, 110.0),
        ("Text", ButtonVariant::Text, 370.0, 90.0),
    ];

    let mut widgets: Vec<Box<dyn Widget>> = Vec::new();

    for (label, variant, x, width) in variants {
        widgets.push(Box::new(
            Button::new(label)
                .variant(variant)
                .position(x, VARIANTS)
                .width(width)
                .on_click(move || println!("clicked: {label}")),
        ));
    }

    for (label, size, x, width) in sizes {
        widgets.push(Box::new(
            Button::new(label)
                .size(size)
                .position(x, SIZES)
                .width(width)
                .on_click(move || println!("clicked: size {label}")),
        ));
    }

    for (label, variant, x, width) in disabled {
        widgets.push(Box::new(
            Button::new(label)
                .variant(variant)
                .position(x, DISABLED)
                .width(width)
                .enabled(false),
        ));
    }

    widgets
}

/// Continuous, discrete and disabled sliders, then one per size.
fn slider_gallery() -> Vec<Box<dyn Widget>> {
    let mut widgets: Vec<Box<dyn Widget>> = Vec::new();

    widgets.push(Box::new(
        Slider::new(0.0, 100.0, 40.0)
            .position(24.0, SLIDERS)
            .width(200.0)
            .labeled(true)
            .on_change(|v| println!("continuous: {v:.1}")),
    ));
    widgets.push(Box::new(
        Slider::new(0.0, 10.0, 3.0)
            .step(1.0)
            .position(256.0, SLIDERS)
            .width(200.0)
            .labeled(true)
            .on_change(|v| println!("discrete: {v}")),
    ));
    widgets.push(Box::new(
        Slider::new(0.0, 100.0, 60.0)
            .position(488.0, SLIDERS)
            .width(200.0)
            .enabled(false),
    ));

    let sizes = [
        (SliderSize::ExtraSmall, 24.0),
        (SliderSize::Small, 24.0),
        (SliderSize::Medium, 24.0),
    ];
    let mut y = SLIDER_SIZES;
    for (size, x) in sizes {
        widgets.push(Box::new(
            Slider::new(0.0, 100.0, 50.0)
                .size(size)
                .position(x, y)
                .width(280.0),
        ));
        y += size.handle_height() + 8.0;
    }

    widgets
}

/// One plain field, one pre-filled field, one disabled field.
fn text_field_gallery() -> Vec<Box<dyn Widget>> {
    vec![
        Box::new(
            TextField::new()
                .position(24.0, TEXT_FIELDS)
                .width(220.0)
                .on_change(|text| println!("text: {text}")),
        ),
        Box::new(
            TextField::new()
                .position(268.0, TEXT_FIELDS)
                .width(220.0)
                .text("フォント入力テスト")
                .on_change(|text| println!("text: {text}")),
        ),
        Box::new(
            TextField::new()
                .position(512.0, TEXT_FIELDS)
                .width(220.0)
                .text("disabled")
                .enabled(false),
        ),
    ]
}

impl App {
    fn new(theme: ColorTheme, fonts: FontBook) -> Self {
        Self {
            theme,
            fonts,
            widgets: button_gallery()
                .into_iter()
                .chain(slider_gallery())
                .chain(text_field_gallery())
                .collect(),
            window: None,
            surface: None,
            cursor: (0.0, 0.0),
            modifiers: keyboard::Modifiers::default(),
            focus: None,
        }
    }

    fn dispatch_pointer(&mut self, kind: PointerEventKind) {
        let event = PointerEvent::new(self.cursor, kind);
        let mut redraw = false;
        for widget in &mut self.widgets {
            redraw |= widget.on_pointer(&event);
        }
        if let PointerEventKind::Press { button } = kind {
            if button == pointer::button::LEFT {
                redraw |= self.update_focus_from_click();
            }
        }
        if redraw {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    /// Topmost (last-drawn) focusable widget under the cursor gets focus;
    /// clicking empty space or a non-focusable widget clears it.
    fn update_focus_from_click(&mut self) -> bool {
        let point = Point::new(self.cursor.0 as f32, self.cursor.1 as f32);
        let hit = self
            .widgets
            .iter()
            .enumerate()
            .rev()
            .find(|(_, w)| w.focusable() && w.bounds().contains(point))
            .map(|(i, _)| i);
        self.set_focus(hit)
    }

    fn set_focus(&mut self, new_focus: Option<usize>) -> bool {
        if new_focus == self.focus {
            return false;
        }
        if let Some(old) = self.focus.and_then(|i| self.widgets.get_mut(i)) {
            old.set_focused(false);
            old.on_keyboard(&KeyboardEvent::new(KeyboardEventKind::Blur, self.modifiers));
        }
        self.focus = new_focus;
        if let Some(new) = self.focus.and_then(|i| self.widgets.get_mut(i)) {
            new.set_focused(true);
            new.on_keyboard(&KeyboardEvent::new(KeyboardEventKind::Focus, self.modifiers));
        }
        true
    }

    /// Delivers only to the focused widget — keyboard events have no
    /// coordinates, so broadcasting would type into every field at once.
    fn dispatch_keyboard(&mut self, kind: KeyboardEventKind) {
        let Some(idx) = self.focus else {
            return;
        };
        let event = KeyboardEvent::new(kind, self.modifiers);
        let redraw = self
            .widgets
            .get_mut(idx)
            .map(|w| w.on_keyboard(&event))
            .unwrap_or(false);
        if redraw {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    /// Tab / Shift+Tab move focus to the next/previous focusable widget,
    /// wrapping around. Does nothing if there are no focusable widgets.
    fn cycle_focus(&mut self, backward: bool) {
        let count = self.widgets.len();
        if count == 0 {
            return;
        }
        let start = self.focus.map(|i| i as isize).unwrap_or(-1);
        let mut i = start;
        for _ in 0..count {
            i = if backward {
                (i - 1).rem_euclid(count as isize)
            } else {
                (i + 1).rem_euclid(count as isize)
            };
            if self.widgets[i as usize].focusable() {
                self.set_focus(Some(i as usize));
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
                return;
            }
        }
        // No focusable widget found; clear focus if one was set.
        if self.set_focus(None) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Rc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("m3_test")
                        .with_inner_size(LogicalSize::new(800.0_f64, 720.0_f64)),
                )
                .expect("failed to create window"),
        );
        let context = Context::new(window.clone()).expect("failed to create softbuffer context");
        let surface =
            Surface::new(&context, window.clone()).expect("failed to create softbuffer surface");
        window.request_redraw();
        self.window = Some(window);
        self.surface = Some(surface);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(_) => {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::CursorEntered { .. } => self.dispatch_pointer(PointerEventKind::Enter),
            WindowEvent::CursorLeft { .. } => self.dispatch_pointer(PointerEventKind::Leave),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                self.dispatch_pointer(PointerEventKind::Motion);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(button) = button_code(button) {
                    let kind = match state {
                        ElementState::Pressed => PointerEventKind::Press { button },
                        ElementState::Released => PointerEventKind::Release { button },
                    };
                    self.dispatch_pointer(kind);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (horizontal, vertical) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (
                        AxisScroll {
                            discrete: x as i32,
                            absolute: x as f64,
                            stop: false,
                        },
                        AxisScroll {
                            discrete: y as i32,
                            absolute: y as f64,
                            stop: false,
                        },
                    ),
                    MouseScrollDelta::PixelDelta(pos) => (
                        AxisScroll {
                            absolute: pos.x,
                            discrete: 0,
                            stop: false,
                        },
                        AxisScroll {
                            absolute: pos.y,
                            discrete: 0,
                            stop: false,
                        },
                    ),
                };
                self.dispatch_pointer(PointerEventKind::Axis {
                    horizontal,
                    vertical,
                });
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.modifiers = keyboard::Modifiers {
                    ctrl: state.control_key(),
                    alt: state.alt_key(),
                    shift: state.shift_key(),
                    logo: state.super_key(),
                    ..Default::default()
                };
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let is_tab_press = event.state == ElementState::Pressed
                    && matches!(&event.logical_key, Key::Named(NamedKey::Tab));

                if is_tab_press {
                    self.cycle_focus(self.modifiers.shift);
                } else {
                    if let Key::Named(named) = &event.logical_key {
                        if let Some(keysym) = keysym_from_named(named) {
                            let kind = match event.state {
                                ElementState::Pressed => KeyboardEventKind::Press {
                                    keysym,
                                    repeat: event.repeat,
                                },
                                ElementState::Released => KeyboardEventKind::Release { keysym },
                            };
                            self.dispatch_keyboard(kind);
                        }
                    }
                    if event.state == ElementState::Pressed {
                        if let Some(text) = event.text.as_deref().and_then(keyboard::insertable_text)
                        {
                            self.dispatch_keyboard(KeyboardEventKind::Commit(text));
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let (Some(window), Some(surface)) = (self.window.as_ref(), self.surface.as_mut())
                else {
                    return;
                };

                let size = window.inner_size();
                let (w, h) = (size.width, size.height);
                if w == 0 || h == 0 {
                    return;
                }

                surface
                    .resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap())
                    .expect("failed to resize surface");

                let mut buffer = surface.buffer_mut().expect("failed to get buffer");

                let bytes: &mut [u8] = unsafe {
                    std::slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut u8, buffer.len() * 4)
                };

                let info = ImageInfo::new_n32_premul((w as i32, h as i32), None);
                let stride = (w * 4) as usize;
                let mut skia_surface = surfaces::wrap_pixels(&info, bytes, stride, None).unwrap();
                let canvas = skia_surface.canvas();

                canvas.clear(Color4f::from(self.theme.surface));

                let mut text_paint = Paint::default();
                text_paint.set_color4f(Color4f::from(self.theme.on_surface), None);
                let font = self.fonts.sized("noto_sans", 24.0);
                canvas.draw_str(
                    "m3_test — ui_core smoke render",
                    Point::new(24.0, 40.0),
                    &font,
                    &text_paint,
                );
                canvas.draw_str(
                    "フォント描画テスト",
                    Point::new(24.0, 68.0),
                    &font,
                    &text_paint,
                );

                let caption = self.fonts.sized("noto_sans", 13.0);
                for (label, row) in [
                    ("Variants", VARIANTS),
                    ("Sizes", SIZES),
                    ("Disabled", DISABLED),
                    ("Sliders", SLIDERS),
                    ("Slider sizes", SLIDER_SIZES),
                    ("Text fields", TEXT_FIELDS),
                ] {
                    canvas.draw_str(
                        label,
                        Point::new(24.0, row - CAPTION_OFFSET),
                        &caption,
                        &text_paint,
                    );
                }

                // draw widgets
                for widget in &mut self.widgets {
                    widget.draw(canvas, &self.theme, &self.fonts);
                }

                // drop skia surface to release the bytes borrow before presenting
                drop(skia_surface);

                buffer.present().expect("failed to present buffer");
            }
            _ => {}
        }
    }
}

fn main() {
    let theme_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "example/theme.toml".to_string());
    let theme = load_theme(&theme_path);

    let mut fonts = FontBook::new();
    fonts.register(
        "noto_sans",
        "Noto Sans CJK JP",
        skia_safe::FontStyle::normal(),
    );

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::new(theme, fonts);
    event_loop.run_app(&mut app).unwrap();
}
