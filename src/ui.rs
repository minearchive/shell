use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use macros::boxed;

use skia_safe::Canvas;
use smithay_client_toolkit::seat::pointer::PointerEvent;

use mpris::Event as MprisEvent;

use ui_core::{animation::parser::Easing, font::FontBook, scheme::ColorTheme};

use crate::{
    components::{clock::Clock, warp::Warp},
    config::{animation::AnimationConfig, theme::Theme},
    dbus::{
        kdeconnect::KDEConnectEvent, mpris::PlayerState, notification::NotificationEvent,
        warp::WarpStatus,
    },
    ipc::events::IPCEvent,
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
    fn on_easing_updated(&mut self, _id: &str, _easing: &Easing) -> Redraw {
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

pub enum UiEvent {
    RequestRedrawAll,
    AnimationUpdated(HashMap<String, Easing>),
}

/// Provider state shared by every screen, so a screen that appears late sees
/// what the others already know.
#[derive(Default)]
pub struct UIState {
    /// Keyed by bus name: two instances of a player share an identity.
    pub players: HashMap<String, PlayerState>,
    pub warp: Option<WarpStatus>,
}

impl UIState {
    pub fn update_player(&mut self, state: PlayerState) {
        if state.active {
            self.players.insert(state.bus_name.clone(), state);
        } else {
            self.players.remove(&state.bus_name);
        }
    }
}

pub struct UserInterface {
    components: Vec<Box<dyn Component>>,
    theme: Arc<RwLock<Theme>>,
}

impl UserInterface {
    pub fn new(theme: Arc<RwLock<Theme>>, animation: &RwLock<AnimationConfig>) -> Self {
        let components: Vec<Box<dyn Component>> = boxed!(Clock::new(animation), Warp);

        Self { components, theme }
    }

    fn fan_out(&mut self, f: impl FnMut(&mut Box<dyn Component>) -> Redraw) -> Redraw {
        self.components.iter_mut().map(f).max().unwrap_or_default()
    }

    pub fn on_ipc(&mut self, event: &IPCEvent) -> Redraw {
        self.fan_out(|c| c.on_ipc(event))
    }

    pub fn on_mpris(&mut self, state: &PlayerState, event: &MprisEvent) -> Redraw {
        self.fan_out(|c| c.on_mpris(state, event))
    }

    pub fn on_kde_connect_event(&mut self, event: &KDEConnectEvent) -> Redraw {
        self.fan_out(|c| c.on_kde_connect_event(event))
    }

    pub fn on_warp(&mut self, status: &WarpStatus) -> Redraw {
        self.fan_out(|c| c.on_warp(status))
    }

    pub fn on_notification(&mut self, event: &NotificationEvent) -> Redraw {
        self.fan_out(|c| c.on_notification(event))
    }

    pub fn draw(&mut self, canvas: &Canvas, state: &UIState, fonts: &FontBook) -> Redraw {
        let cfg = self.theme.read().unwrap();
        let theme = cfg.theme();
        canvas.clear(theme.surface_container);

        let redraw = self
            .components
            .iter_mut()
            .map(|c| c.draw(canvas, state, fonts, theme))
            .max()
            .unwrap_or_default();
        redraw
    }

    pub fn on_cursor(&mut self, event: &PointerEvent) -> Redraw {
        self.fan_out(|c| c.on_cursor(event))
    }

    pub fn on_easing_updated(&mut self, id: &str, easing: &Easing) -> Redraw {
        self.fan_out(|c| c.on_easing_updated(id, easing))
    }

    pub fn on_drag_enter(&mut self, mime_types: &[String]) -> Redraw {
        self.fan_out(|c| c.on_drag_enter(mime_types))
    }

    pub fn on_drag_motion(&mut self, x: f64, y: f64) -> Redraw {
        self.fan_out(|c| c.on_drag_motion(x, y))
    }

    pub fn on_drag_leave(&mut self) -> Redraw {
        self.fan_out(|c| c.on_drag_leave())
    }

    pub fn on_drop(&mut self, mime: &str, data: &[u8]) -> Redraw {
        self.fan_out(|c| c.on_drop(mime, data))
    }
}
