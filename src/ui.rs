use calloop::channel::Sender;
use mpris::Event;
use skia_safe::{utils::text_utils::Align, Canvas, Color4f, Paint};
use smithay_client_toolkit::seat::{
    keyboard::{KeyEvent, Keysym, Modifiers},
    pointer::{PointerEvent, PointerEventKind},
};

use skia_safe::FontStyle;

use crate::{
    dbus::mpris::MprisClient,
    font::Fonts,
    ipc::{events::IPCEvent, IpcTrait, WindowManagerIPC},
    KeyTiming,
};

pub trait Component {
    fn draw(&self, canvas: &Canvas, state: &UIState);
    fn on_cursor(&self, events: &PointerEvent);
    fn on_key(&mut self, event: &KeyEvent, timing: &KeyTiming);
    fn on_ipc(&mut self, events: &IPCEvent);
    fn on_mpris(&mut self, event: &Event);
}

pub enum UiEvent {
    RequestRedraw(usize),
}

pub struct UIState {
    pub ipc: WindowManagerIPC,
    pub mpris: MprisClient,
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
    pub fn new(ipc_sender: Sender<IPCEvent>, mpris_sender: Sender<Event>) -> Self {
        let mut s = Self {
            ipc: WindowManagerIPC::new(ipc_sender),
            mpris: MprisClient::new(mpris_sender),
            workspace_id: "".into(),
            window_title: "".into(),
            padding: 0.,
            shoud_redraw: true,
        };

        s.workspace_id = s.ipc.get_current_workspace().to_string();
        s.window_title = s.ipc.get_current_window_name().unwrap_or_default();

        s
    }
}

impl UserInterface {
    pub fn new(
        rx: Sender<UiEvent>,
        ipc_sender: Sender<IPCEvent>,
        mpris_sender: Sender<Event>,
        idx: usize,
    ) -> Self {
        Self {
            components: Vec::new(),
            idx,
            modifier: Modifiers::default(),
            state: UIState::new(ipc_sender, mpris_sender),
            sender: rx,
            font: Fonts::roboto(FontStyle::normal()),
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
            IPCEvent::FocusedWindowChanged(title) => {
                self.state.shoud_redraw = true;
                self.state.window_title = title.unwrap_or_default();
                let _ = self.sender.send(UiEvent::RequestRedraw(self.idx));
            }
        }
    }

    pub fn on_mpris(&mut self, event: Event) {
        self.components.iter_mut().for_each(|c| c.on_mpris(&event));

        match event {
            Event::PlayerShutDown => todo!(),
            Event::Paused => todo!(),
            Event::Playing => todo!(),
            Event::Stopped => todo!(),
            Event::LoopingChanged(loop_status) => todo!(),
            Event::ShuffleToggled(_) => todo!(),
            Event::VolumeChanged(_) => todo!(),
            Event::PlaybackRateChanged(_) => todo!(),
            Event::TrackChanged(metadata) => todo!(),
            Event::Seeked { position_in_us } => todo!(),
            Event::TrackAdded(track_id) => todo!(),
            Event::TrackRemoved(track_id) => todo!(),
            Event::TrackMetadataChanged { old_id, new_id } => todo!(),
            Event::TrackListReplaced => todo!(),
        }
    }

    pub fn draw(&mut self, canvas: &Canvas) {
        self.components
            .iter()
            .for_each(|c| c.draw(canvas, &self.state));

        let font = self.font.sized(32.);
        let mut paint = Paint::default();

        canvas.clear(Color4f::new(1., 1., 1., 1.));
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
