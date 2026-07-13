use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use calloop::channel::Sender;
use skia_safe::{Canvas, FontStyle};
use smithay_client_toolkit::seat::{keyboard::Modifiers, pointer::PointerEvent};

use mpris::Event as MprisEvent;

use crate::{
    animation::parser::Easing,
    components::{clock::Clock, warp::Warp},
    config::{animation::AnimationConfig, config::Configuration},
    dbus::{
        kdeconnect::KDEConnectEvent, mpris::PlayerState, notification::NotificationEvent,
        warp::WarpStatus,
    },
    font::FontBook,
    ipc::{events::IPCEvent, WindowManagerIPC},
    Commands,
};

pub trait Component {
    fn draw(&mut self, canvas: &Canvas, state: &UIState, fonts: &FontBook);
    fn on_cursor(&mut self, _events: &PointerEvent) {}
    // fn on_key(&mut self, event: &KeyEvent, timing: &KeyTiming);
    fn on_ipc(&mut self, _events: &IPCEvent) {}
    fn on_mpris(&mut self, _state: &PlayerState, _event: &MprisEvent) {}
    fn on_kde_connect_event(&mut self, _event: &KDEConnectEvent) {}
    fn on_warp(&mut self, _status: &WarpStatus) {}
    fn on_notification(&mut self, _event: &NotificationEvent) {}
    fn on_easing_updated(&mut self, _id: String, _easing: &Easing) {}
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
    config: Arc<RwLock<Configuration>>,
    _sender: Sender<UiEvent>,
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
        config: Arc<RwLock<Configuration>>,
        animation: Arc<RwLock<AnimationConfig>>,
    ) -> Self {
        let components: Vec<Box<dyn Component>> = vec![
            Box::new(Clock::new(
                rx.clone(),
                idx,
                1000,
                Arc::clone(&config),
                Arc::clone(&animation),
            )),
            Box::new(Warp::new(rx.clone(), Arc::clone(&config), commands.warp)),
        ];

        Self {
            components,
            _idx: idx,
            modifier: Modifiers::default(),
            state: UIState::new(),
            config,
            _sender: rx,
        }
    }

    pub fn on_ipc(&mut self, event: IPCEvent) {
        self.components.iter_mut().for_each(|c| c.on_ipc(&event));
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

    pub fn on_kde_connect_event(&mut self, event: &KDEConnectEvent) {
        self.components
            .iter_mut()
            .for_each(|c| c.on_kde_connect_event(event));
    }

    pub fn on_warp(&mut self, status: &WarpStatus) {
        self.components.iter_mut().for_each(|c| c.on_warp(status));
        self.state.warp = Some(status.clone());
    }

    pub fn on_notification(&mut self, event: NotificationEvent) {
        self.components
            .iter_mut()
            .for_each(|c| c.on_notification(&event));
    }

    pub fn draw(&mut self, canvas: &Canvas, fonts: &FontBook) {
        let cfg = self.config.read().unwrap();
        canvas.clear(cfg.theme().surface_container);

        self.components
            .iter_mut()
            .for_each(|c| c.draw(canvas, &self.state, fonts));
    }

    pub fn on_cursor(&mut self, event: &PointerEvent) {
        self.components.iter_mut().for_each(|c| c.on_cursor(event));
    }

    pub fn on_modifier(&mut self, modifier: Modifiers) {
        self.modifier = modifier;
    }

    pub fn on_easing_updated(&mut self, id: String, easing: Easing) {
        self.components
            .iter_mut()
            .for_each(|c| c.on_easing_updated(id.clone(), &easing));
    }
}
