use calloop::channel::Sender;
use skia_safe::{utils::text_utils::Align, Canvas, Color4f, Font, FontMgr, FontStyle, Paint};
use smithay_client_toolkit::seat::{
    keyboard::{KeyEvent, Keysym, Modifiers},
    pointer::{PointerEvent, PointerEventKind},
};

use crate::{
    ipc::{IpcTrait, WindowManagerIPC},
    KeyTiming,
};

pub trait Component {
    fn draw(&self, canvas: &Canvas, state: &UIState);
}

pub enum UiEvent {
    RequestRedraw(usize),
}

pub struct UIState {
    pub ipc: WindowManagerIPC,
    pub current_window_name: String,
    pub padding: f32,
    pub shoud_redraw: bool,
}

pub struct UserInterface {
    pub components: Vec<Box<dyn Component>>,
    idx: usize,
    modifier: Modifiers,
    state: UIState,
    sender: Sender<UiEvent>,
}

impl UIState {
    pub fn new() -> Self {
        Self {
            ipc: WindowManagerIPC::default(),
            current_window_name: "THIS IS EXAMPLE TEXT".to_string(),
            padding: 0.,
            shoud_redraw: true,
        }
    }
}

impl UserInterface {
    pub fn new(rx: Sender<UiEvent>, idx: usize) -> Self {
        Self {
            components: Vec::new(),
            idx,
            modifier: Modifiers::default(),
            state: UIState::new(),
            sender: rx,
        }
    }

    pub fn draw(&mut self, canvas: &Canvas) {
        self.components
            .iter()
            .for_each(|c| c.draw(&canvas, &self.state));

        self.state.current_window_name = self
            .state
            .ipc
            .get_current_window_name()
            .unwrap_or("Unkonow".to_string());

        let font_mgr = FontMgr::new();

        let typeface = font_mgr
            .legacy_make_typeface("Unifont", FontStyle::normal())
            .expect("Failed to create typeface");

        let font = Font::new(typeface, 32.);
        let mut paint = Paint::default();

        canvas.clear(Color4f::new(1., 1., 1., 1.));
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::new(0., 0., 0., 1.), None);

        let font_metrics = font.metrics();

        let y = -font_metrics.1.ascent;

        canvas.draw_str_align(
            self.state.current_window_name.to_string(),
            (self.state.padding, y),
            &font,
            &paint,
            Align::Left,
        );
    }

    pub fn on_key(&mut self, event: &KeyEvent, timing: &KeyTiming) {
        if event.keysym == Keysym::KP_Space {
            match timing {
                KeyTiming::Press => println!("Space Pressed"),
                KeyTiming::Repeat => println!("Space Repeating"),
                KeyTiming::Release => println!("Space Released"),
            }
        }
    }

    pub fn on_cursor(&mut self, event: &PointerEvent) {
        match event.kind {
            PointerEventKind::Axis {
                horizontal,
                vertical,
                ..
            } => {
                if horizontal.absolute != 0. {
                    self.state.padding -= horizontal.absolute as f32;
                    self.state.shoud_redraw = true;
                    let _ = self.sender.send(UiEvent::RequestRedraw(self.idx));
                }

                if vertical.absolute != 0. && self.modifier.shift {
                    self.state.padding -= vertical.absolute as f32;
                    self.state.shoud_redraw = true;
                    let _ = self.sender.send(UiEvent::RequestRedraw(self.idx));
                }
            }
            _ => {}
        }
    }

    pub fn should_redraw(&mut self) -> bool {
        let redraw = self.state.shoud_redraw;
        self.state.shoud_redraw = false;
        redraw
    }

    pub fn on_modifier(&mut self, modifier: Modifiers) {
        self.modifier = modifier;
    }
}
