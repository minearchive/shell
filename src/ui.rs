use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use macros::boxed;

use calloop::channel::Sender;
use skia_safe::{Canvas, FontStyle};
use smithay_client_toolkit::seat::{keyboard::Modifiers, pointer::PointerEvent};

use mpris::Event as MprisEvent;

use crate::{
    animation::parser::Easing,
    components::{clock::Clock, warp::Warp},
    config::{animation::AnimationConfig, scheme::ColorTheme, theme::Theme},
    dbus::{
        kdeconnect::KDEConnectEvent, mpris::PlayerState, notification::NotificationEvent,
        warp::WarpStatus,
    },
    font::FontBook,
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
            .fold(Redraw::None, Redraw::max)
    }

    pub fn on_mpris(&mut self, state: &PlayerState, event: &MprisEvent) -> Redraw {
        let redraw = self
            .components
            .iter_mut()
            .map(|c| c.on_mpris(state, event))
            .fold(Redraw::None, Redraw::max);

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
            .fold(Redraw::None, Redraw::max)
    }

    pub fn on_warp(&mut self, status: &WarpStatus) -> Redraw {
        let redraw = self
            .components
            .iter_mut()
            .map(|c| c.on_warp(status))
            .fold(Redraw::None, Redraw::max);
        self.state.warp = Some(status.clone());
        redraw
    }

    pub fn on_notification(&mut self, event: NotificationEvent) -> Redraw {
        self.components
            .iter_mut()
            .map(|c| c.on_notification(&event))
            .fold(Redraw::None, Redraw::max)
    }

    pub fn draw(&mut self, canvas: &Canvas, fonts: &FontBook) {
        let cfg = self.theme.read().unwrap();
        let theme = cfg.theme();
        canvas.clear(theme.surface_container);

        let redraw = self
            .components
            .iter_mut()
            .map(|c| c.draw(canvas, &self.state, fonts, theme))
            .fold(Redraw::None, Redraw::max);

        if redraw == Redraw::Animating {
            let _ = self.sender.send(UiEvent::RequestRedrawAll);
        }
    }

    pub fn on_cursor(&mut self, event: &PointerEvent) -> Redraw {
        self.components
            .iter_mut()
            .map(|c| c.on_cursor(event))
            .fold(Redraw::None, Redraw::max)
    }

    pub fn on_modifier(&mut self, modifier: Modifiers) {
        self.modifier = modifier;
    }

    pub fn on_easing_updated(&mut self, id: String, easing: Easing) -> Redraw {
        self.components
            .iter_mut()
            .map(|c| c.on_easing_updated(id.clone(), &easing))
            .fold(Redraw::None, Redraw::max)
    }
}
