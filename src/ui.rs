use std::collections::HashMap;

use calloop::channel::Sender;
use skia_safe::{utils::text_utils::Align, Canvas, Color4f, Paint};
use smithay_client_toolkit::seat::{
    keyboard::{KeyEvent, Keysym, Modifiers},
    pointer::{PointerEvent, PointerEventKind},
};

use skia_safe::FontStyle;

use mpris::Event as MprisEvent;

use crate::{
    components::clock::Clock,
    dbus::mpris::PlayerState,
    font::Fonts,
    ipc::{events::IPCEvent, IpcTrait, WindowManagerIPC},
    KeyTiming,
};

pub trait Component {
    fn draw(&self, canvas: &Canvas, state: &UIState);
    fn on_cursor(&self, events: &PointerEvent);
    fn on_key(&mut self, event: &KeyEvent, timing: &KeyTiming);
    fn on_ipc(&mut self, events: &IPCEvent);
    fn on_mpris(&mut self, state: &PlayerState, event: &MprisEvent);
}

pub enum UiEvent {
    RequestRedraw(usize),
}

pub struct UIState {
    pub players: HashMap<String, PlayerState>,
    pub workspace_id: String,
    pub window_title: String,
    pub padding: f32,
    pub shoud_redraw: bool,
}

pub struct UserInterface {
    pub components: Vec<Box<dyn Component>>,
    idx: usize,
    modifier: Modifiers,
    state: UIState,
    sender: Sender<UiEvent>,
    font: Fonts,
}

impl UIState {
    pub fn new(ipc: &mut WindowManagerIPC) -> Self {
        Self {
            players: HashMap::new(),
            workspace_id: ipc.get_current_workspace().to_string(),
            window_title: ipc.get_current_window_name().unwrap_or_default(),
            padding: 0.,
            shoud_redraw: true,
        }
    }
}

impl UserInterface {
    pub fn new(rx: Sender<UiEvent>, idx: usize, ipc: &mut WindowManagerIPC) -> Self {
        let components: Vec<Box<dyn Component>> = vec![Box::new(Clock::new(rx.clone(), idx, 1000))];

        Self {
            components,
            idx,
            modifier: Modifiers::default(),
            state: UIState::new(ipc),
            sender: rx,
            font: Fonts::noto_sans(FontStyle::normal()),
        }
    }

    pub fn on_ipc(&mut self, event: IPCEvent) {
        self.components.iter_mut().for_each(|c| c.on_ipc(&event));

        match event {
            IPCEvent::ForcusedWorkspaceChanged(_old, new) => {
                self.state.shoud_redraw = true;
                self.state.workspace_id = new.to_string();
                let _ = self.sender.send(UiEvent::RequestRedraw(self.idx));
            }
            IPCEvent::FocusedWindowTitleChanged(title) => {
                self.state.shoud_redraw = true;
                self.state.window_title = title.unwrap_or_default();
                let _ = self.sender.send(UiEvent::RequestRedraw(self.idx));
            }
        }
    }

    pub fn on_mpris(&mut self, state: &PlayerState, event: &MprisEvent) {
        self.components
            .iter_mut()
            .for_each(|c| c.on_mpris(state, event));

        if state.active {
            self.state
                .players
                .insert(state.identity.clone(), state.clone());
        } else {
            self.state.players.remove(&state.identity);
        }
    }

    pub fn draw(&mut self, canvas: &Canvas) {
        canvas.clear(Color4f::new(1., 1., 1., 1.));

        self.components
            .iter()
            .for_each(|c| c.draw(canvas, &self.state));

        let font = self.font.sized(32.);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::new(0., 0., 0., 1.), None);

        let font_metrics = font.metrics();

        let y = -font_metrics.1.ascent;

        canvas.draw_str_align(
            &self.state.workspace_id,
            (self.state.padding, y),
            &font,
            &paint,
            Align::Left,
        );

        canvas.draw_str_align(
            &self.state.window_title,
            (self.state.padding + 64., y),
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

        self.components
            .iter_mut()
            .for_each(|c| c.on_key(event, timing));
    }

    pub fn on_cursor(&mut self, event: &PointerEvent) {
        #[allow(unused)]
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
            PointerEventKind::Enter { serial } => {}
            PointerEventKind::Leave { serial } => {}
            PointerEventKind::Motion { time } => {}
            PointerEventKind::Press {
                time,
                button,
                serial,
            } => {}
            PointerEventKind::Release {
                time,
                button,
                serial,
            } => {}
        }

        self.components.iter_mut().for_each(|c| c.on_cursor(event));
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
