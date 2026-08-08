use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use macros::boxed;

use calloop::channel::Sender;
use skia_safe::{Canvas, FontStyle};
use smithay_client_toolkit::seat::{keyboard::Modifiers, pointer::PointerEvent};

use mpris::Event as MprisEvent;

use ui_core::{animation::parser::Easing, font::FontBook, scheme::ColorTheme};

use crate::{
    components::{clock::Clock, warp::Warp},
    config::{animation::AnimationConfig, theme::Theme},
    dbus::{
        kdeconnect::KDEConnectEvent, mpris::PlayerState, notification::NotificationEvent,
        warp::WarpStatus,
    },
    ipc::{events::IPCEvent, WindowManagerIPC},
    Commands,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum Redraw {
    #[default]
    None,
    Now,
    Animating,
}

pub trait Component {
    fn draw(
        &mut self,
        canvas: &Canvas,
        state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> Redraw;
    fn on_cursor(&mut self, _events: &PointerEvent) -> Redraw {
        Redraw::None
    }
    fn on_ipc(&mut self, _events: &IPCEvent) -> Redraw {
        Redraw::None
    }
    fn on_mpris(&mut self, _state: &PlayerState, _event: &MprisEvent) -> Redraw {
        Redraw::None
    }
    fn on_kde_connect_event(&mut self, _event: &KDEConnectEvent) -> Redraw {
        Redraw::None
    }
    fn on_warp(&mut self, _status: &WarpStatus) -> Redraw {
        Redraw::None
    }
    fn on_notification(&mut self, _event: &NotificationEvent) -> Redraw {
        Redraw::None
    }
    fn on_easing_updated(&mut self, _id: String, _easing: &Easing) -> Redraw {
        Redraw::None
    }
    fn on_drag_enter(&mut self, _mime_types: &[String]) -> Redraw {
        Redraw::None
    }
    fn on_drag_motion(&mut self, _x: f64, _y: f64) -> Redraw {
        Redraw::None
    }
    fn on_drag_leave(&mut self) -> Redraw {
        Redraw::None
    }
    fn on_drop(&mut self, _mime: &str, _data: &[u8]) -> Redraw {
        Redraw::None
    }
}

#[allow(unused)]
pub enum UiEvent {
    RequestRedraw(usize),
    RegisterFont(String, String, FontStyle),
    RequestRedrawAll,
    AnimationUpdated(HashMap<String, Easing>),
}

pub struct UIState {
    pub players: HashMap<String, PlayerState>,
    pub warp: Option<WarpStatus>,
}

pub struct UserInterface {
    pub components: Vec<Box<dyn Component>>,
    _idx: usize,
    modifier: Modifiers,
    state: UIState,
    theme: Arc<RwLock<Theme>>,
    sender: Sender<UiEvent>,
}

impl UIState {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            warp: None,
        }
    }
}

impl UserInterface {
    pub fn new(
        rx: Sender<UiEvent>,
        idx: usize,
        _ipc: &mut WindowManagerIPC,
        commands: Commands,
        theme: Arc<RwLock<Theme>>,
        animation: Arc<RwLock<AnimationConfig>>,
    ) -> Self {
        let components: Vec<Box<dyn Component>> = boxed!(
            Clock::new(rx.clone(), idx, 1000, Arc::clone(&animation)),
            Warp::new(commands.warp)
        );

        Self {
            components,
            _idx: idx,
            modifier: Modifiers::default(),
            state: UIState::new(),
            theme,
            sender: rx,
        }
    }

    pub fn on_ipc(&mut self, event: IPCEvent) -> Redraw {
        self.components
            .iter_mut()
            .map(|c| c.on_ipc(&event))
            .max()
            .unwrap_or_default()
    }

    pub fn on_mpris(&mut self, state: &PlayerState, event: &MprisEvent) -> Redraw {
        let redraw = self
            .components
            .iter_mut()
            .map(|c| c.on_mpris(state, event))
            .max()
            .unwrap_or_default();

        if state.active {
            self.state
                .players
                .insert(state.identity.clone(), state.clone());
        } else {
            self.state.players.remove(&state.identity);
        }

        redraw
    }

    pub fn on_kde_connect_event(&mut self, event: &KDEConnectEvent) -> Redraw {
        self.components
            .iter_mut()
            .map(|c| c.on_kde_connect_event(event))
            .max()
            .unwrap_or_default()
    }

    pub fn on_warp(&mut self, status: &WarpStatus) -> Redraw {
        let redraw = self
            .components
            .iter_mut()
            .map(|c| c.on_warp(status))
            .max()
            .unwrap_or_default();
        self.state.warp = Some(status.clone());
        redraw
    }

    pub fn on_notification(&mut self, event: NotificationEvent) -> Redraw {
        self.components
            .iter_mut()
            .map(|c| c.on_notification(&event))
            .max()
            .unwrap_or_default()
    }

    pub fn draw(&mut self, canvas: &Canvas, fonts: &FontBook) {
        let cfg = self.theme.read().unwrap();
        let theme = cfg.theme();
        canvas.clear(theme.surface_container);

        let redraw = self
            .components
            .iter_mut()
            .map(|c| c.draw(canvas, &self.state, fonts, theme))
            .max()
            .unwrap_or_default();

        if redraw == Redraw::Animating {
            let _ = self.sender.send(UiEvent::RequestRedrawAll);
        }
    }

    pub fn on_cursor(&mut self, event: &PointerEvent) -> Redraw {
        self.components
            .iter_mut()
            .map(|c| c.on_cursor(event))
            .max()
            .unwrap_or_default()
    }

    pub fn on_modifier(&mut self, modifier: Modifiers) {
        self.modifier = modifier;
    }

    pub fn on_easing_updated(&mut self, id: String, easing: Easing) -> Redraw {
        self.components
            .iter_mut()
            .map(|c| c.on_easing_updated(id.clone(), &easing))
            .max()
            .unwrap_or_default()
    }

    pub fn on_drag_enter(&mut self, mime_types: &[String]) -> Redraw {
        self.components
            .iter_mut()
            .map(|c| c.on_drag_enter(mime_types))
            .max()
            .unwrap_or_default()
    }

    pub fn on_drag_motion(&mut self, x: f64, y: f64) -> Redraw {
        self.components
            .iter_mut()
            .map(|c| c.on_drag_motion(x, y))
            .max()
            .unwrap_or_default()
    }

    pub fn on_drag_leave(&mut self) -> Redraw {
        self.components
            .iter_mut()
            .map(|c| c.on_drag_leave())
            .max()
            .unwrap_or_default()
    }

    pub fn on_drop(&mut self, mime: &str, data: &[u8]) -> Redraw {
        self.components
            .iter_mut()
            .map(|c| c.on_drop(mime, data))
            .max()
            .unwrap_or_default()
    }
}
