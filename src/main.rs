use std::{
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use calloop::{
    channel::{self, Channel, Sender},
    timer::{TimeoutAction, Timer},
    EventLoop, LoopHandle,
};
use calloop_wayland_source::WaylandSource;
use log::{debug, info, warn};
use skia_safe::{surfaces, ImageInfo};
use smithay_client_toolkit::{
    compositor::{CompositorState, Region},
    data_device_manager::{data_device::DataDevice, data_offer::DragOffer, DataDeviceManagerState},
    delegate_compositor, delegate_data_device, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::OutputState,
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::SeatState,
    shell::{
        wlr_layer::{LayerShell, LayerSurface},
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};

use tokio::sync::mpsc::UnboundedSender;
use wayland_client::{
    globals::registry_queue_init,
    protocol::{
        wl_output::WlOutput, wl_pointer::WlPointer, wl_seat::WlSeat, wl_shm, wl_surface::WlSurface,
    },
    Connection, QueueHandle,
};

use mpris::Event as MprisEvent;

use crate::{
    config::{animation::AnimationConfig, config_path, theme::Theme, WatchableConfig},
    dbus::{
        kdeconnect::{KDEConnectClient, KDEConnectCommand, KDEConnectEvent},
        mpris::PlayerState,
        notification::{CloseReason, NotificationEvent, NotificationHandle},
        warp::{WarpClient, WarpCommand, WarpStatus},
    },
    ipc::events::IPCEvent,
    system::{SystemCommand, SystemStatus},
    ui::{Redraw, UIState, UiEvent, UserInterface, BAR_HEIGHT, ISLAND_MAX_HEIGHT},
};

use ui_core::font::FontBook;

mod catcher;
mod components;
mod config;
mod dbus;
mod dnd;
mod ipc;
mod output;
mod seat;
mod system;

mod ui;

const CLOCK_TICK: Duration = Duration::from_secs(1);

/// Dropping a sender closes its provider's command channel, so these live as
/// long as the bar even though nothing issues commands yet.
#[allow(unused)]
pub struct Commands {
    kdeconnect: UnboundedSender<KDEConnectCommand>,
}

pub struct Screen {
    layer: LayerSurface,
    width: u32,
    height: u32,
    first_configure: bool,
    needs_redraw: bool,
    frame_pending: bool,
    /// What the input region was last set from: `[width, ..island rect]`.
    input_region: [i32; 5],
    ui: UserInterface,
    output: WlOutput,
    catcher: Option<catcher::Catcher>,
}

pub struct Shell {
    pool: SlotPool,
    output_state: OutputState,
    seat_state: SeatState,
    compositor_state: CompositorState,
    data_device_manager_state: DataDeviceManagerState,
    layer_shell: LayerShell,
    qh: QueueHandle<Shell>,
    loop_handle: LoopHandle<'static, Shell>,
    data_device: Option<DataDevice>,
    dragging: Option<WlSurface>,
    pointer: Option<WlPointer>,
    screen: Vec<Screen>,
    shm: Shm,
    registry_state: RegistryState,
    font: FontBook,
    state: UIState,
    theme: Arc<RwLock<Theme>>,
    animation_config: Arc<RwLock<AnimationConfig>>,
    ui_tx: Sender<UiEvent>,
    system_commands: UnboundedSender<SystemCommand>,
    warp_commands: UnboundedSender<WarpCommand>,
    notification_handle: Option<NotificationHandle>,
    _commands: Commands,
}

/// Run `f` on the event loop for every message that arrives on `rx`.
fn on_msg<T: 'static>(
    handle: &LoopHandle<'static, Shell>,
    rx: Channel<T>,
    mut f: impl FnMut(T, &mut Shell) + 'static,
) {
    handle
        .insert_source(rx, move |event, _, shell| {
            if let channel::Event::Msg(msg) = event {
                f(msg, shell);
            }
        })
        .unwrap();
}

fn main() {
    env_logger::init();

    let connection = Connection::connect_to_env().expect("Failed to connect to env");

    let (globals, event_queue) = registry_queue_init(&connection).unwrap();
    let qh: QueueHandle<Shell> = event_queue.handle();

    let mut event_loop: EventLoop<Shell> = EventLoop::try_new().unwrap();
    let loop_handle = event_loop.handle();

    WaylandSource::new(connection, event_queue)
        .insert(loop_handle.clone())
        .unwrap();

    let (notification_tx, notification_rx) = channel::channel::<NotificationEvent>();
    // Losing the name to another notification daemon costs only this feature.
    let notification_handle = NotificationHandle::init(notification_tx)
        .map_err(|e| warn!("Notification server unavailable: {e}"))
        .ok();
    on_msg(&loop_handle, notification_rx, |event, shell| {
        debug!("{event:?}");
        shell.state.notifications.apply(&event, Instant::now());
        shell.dispatch(|ui| ui.on_notification(&event));
    });

    let (mpris_tx, mpris_rx) = channel::channel::<(PlayerState, MprisEvent)>();
    dbus::mpris::start_listener(mpris_tx);
    on_msg(&loop_handle, mpris_rx, |(state, event), shell| {
        shell.state.update_player(state.clone());
        shell.dispatch(|ui| ui.on_mpris(&state, &event));
    });

    let (ipc_tx, ipc_rx) = channel::channel::<IPCEvent>();
    ipc::listen(ipc_tx);
    on_msg(&loop_handle, ipc_rx, |event, shell| {
        shell.dispatch(|ui| ui.on_ipc(&event));
    });

    let (kde_tx, kde_rx) = channel::channel::<KDEConnectEvent>();
    let kde_command_sender = KDEConnectClient::init(kde_tx);
    on_msg(&loop_handle, kde_rx, |event, shell| {
        shell.dispatch(|ui| ui.on_kde_connect_event(&event));
    });

    let (warp_tx, warp_rx) = channel::channel::<WarpStatus>();
    let warp_commands = WarpClient::init(warp_tx);
    on_msg(&loop_handle, warp_rx, |status, shell| {
        shell.state.warp = Some(status.clone());
        shell.dispatch(|ui| ui.on_warp(&status));
    });

    let (system_tx, system_rx) = channel::channel::<SystemStatus>();
    let system_commands = system::start(system_tx);
    on_msg(&loop_handle, system_rx, |status, shell| {
        shell.state.system = status;
        shell.request_redraw_all();
    });

    let (ui_tx, ui_rx) = channel::channel::<UiEvent>();
    on_msg(&loop_handle, ui_rx, |msg, shell| match msg {
        UiEvent::RequestRedrawAll => shell.request_redraw_all(),
        UiEvent::AnimationUpdated(easings) => shell.dispatch(|ui| {
            easings
                .iter()
                .map(|(id, easing)| ui.on_easing_updated(id, easing))
                .max()
                .unwrap_or_default()
        }),
        UiEvent::DismissNotification(id) => {
            if shell.state.notifications.remove(id) {
                shell.notify_closed(id, CloseReason::Dismissed);
                shell.request_redraw_all();
            }
        }
    });

    loop_handle
        .insert_source(Timer::from_duration(CLOCK_TICK), |_, _, shell| {
            // ponytail: expiry is only as precise as this tick. Give it its own
            // timer if a notification ever needs sub-second timing.
            for id in shell.state.notifications.take_expired(Instant::now()) {
                shell.notify_closed(id, CloseReason::Expired);
            }
            shell.request_redraw_all();
            TimeoutAction::ToDuration(CLOCK_TICK)
        })
        .unwrap();

    let compositor = CompositorState::bind(&globals, &qh).expect("Wayland compositor not found");

    let layer_shell =
        LayerShell::bind(&globals, &qh).expect("Wayland layer shell is not available");

    let data_device_manager_state = DataDeviceManagerState::bind(&globals, &qh)
        .expect("Data Device Manager State is not available");

    let shm = Shm::bind(&globals, &qh).expect("Wayland shared memory is not available");

    let pool = SlotPool::new(256 * 256 * 4, &shm).expect("failed to create pool");

    let theme = Theme::load_and_watch(&config_path("theme.toml"), ui_tx.clone());

    let animation_config =
        AnimationConfig::load_and_watch(&config_path("animation.toml"), ui_tx.clone());

    let mut application = Shell {
        pool,
        output_state: OutputState::new(&globals, &qh),
        seat_state: SeatState::new(&globals, &qh),
        compositor_state: compositor,
        data_device_manager_state,
        qh: qh.clone(),
        loop_handle,
        layer_shell,
        pointer: None,
        screen: Vec::new(),
        shm,
        registry_state: RegistryState::new(&globals),
        font: {
            let mut book = FontBook::new();
            book.register(
                "noto_sans",
                "Noto Sans CJK JP",
                skia_safe::FontStyle::normal(),
            );
            book
        },
        state: UIState::default(),
        theme,
        animation_config,
        ui_tx,
        system_commands,
        warp_commands,
        notification_handle,
        _commands: Commands {
            kdeconnect: kde_command_sender,
        },
        data_device: None,
        dragging: None,
    };

    let seats: Vec<_> = application.seat_state.seats().collect();
    for seat in &seats {
        application.ensure_data_device(&qh, seat);
    }

    event_loop.run(None, &mut application, |_| {}).unwrap();
}

impl ShmHandler for Shell {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl Shell {
    /// Create the seat's `wl_data_device`, without which the compositor has
    /// nowhere to send drag-and-drop events.
    ///
    /// Called both from `SeatHandler::new_seat` and, at startup, for the seats
    /// `SeatState::new` already bound — SCTK only reaches `new_seat` through
    /// `RegistryHandler::new_global`, which is not called during the initial
    /// enumeration of globals.
    fn ensure_data_device(&mut self, qh: &QueueHandle<Self>, seat: &WlSeat) {
        if self.data_device.is_some() {
            return;
        }
        info!("Creating data device for seat");
        self.data_device = Some(self.data_device_manager_state.get_data_device(qh, seat));
    }

    fn screen_for(&self, surface: &WlSurface) -> Option<usize> {
        self.screen
            .iter()
            .position(|s| s.layer.wl_surface() == surface)
    }

    /// Fan an input event out to every screen's UI and repaint the ones that
    /// asked for it.
    fn dispatch(&mut self, mut f: impl FnMut(&mut UserInterface) -> Redraw) {
        for i in 0..self.screen.len() {
            if f(&mut self.screen[i].ui) != Redraw::None {
                self.request_redraw(i);
            }
        }
    }

    /// Send an event to the UI on `surface`, if that screen still exists.
    fn dispatch_to(&mut self, surface: &WlSurface, f: impl FnOnce(&mut UserInterface) -> Redraw) {
        let Some(idx) = self.screen_for(surface) else {
            return;
        };
        if f(&mut self.screen[idx].ui) != Redraw::None {
            self.request_redraw(idx);
        }
    }

    /// Tell the notification's sender it is gone.
    fn notify_closed(&self, id: u32, reason: CloseReason) {
        if let Some(handle) = &self.notification_handle {
            if let Err(e) = handle.close(id, reason) {
                warn!("Failed to signal notification {id} closed: {e}");
            }
        }
    }

    /// Limit input to what is visible, so the headroom above the bar clicks
    /// through to the windows underneath.
    fn update_input_region(&mut self, idx: usize, width: i32) {
        let island = self.screen[idx].ui.island_rect().map_or([0; 4], |r| {
            [
                r.x.floor() as i32,
                r.y.floor() as i32,
                r.width.ceil() as i32,
                r.height.ceil() as i32,
            ]
        });
        let current = [width, island[0], island[1], island[2], island[3]];
        if self.screen[idx].input_region == current {
            return;
        }
        let Ok(region) = Region::new(&self.compositor_state) else {
            return;
        };
        region.add(0, ISLAND_MAX_HEIGHT as i32, width, BAR_HEIGHT as i32);
        region.add(island[0], island[1], island[2], island[3]);
        let surface = self.screen[idx].layer.wl_surface();
        surface.set_input_region(Some(region.wl_region()));
        self.screen[idx].input_region = current;
    }

    /// The drag offer currently being held over this bar, if any.
    fn drag_offer(&self) -> Option<DragOffer> {
        self.data_device
            .as_ref()
            .and_then(|d| d.data().drag_offer())
    }

    pub fn request_redraw_all(&mut self) {
        for i in 0..self.screen.len() {
            self.request_redraw(i);
        }
    }

    pub fn request_redraw(&mut self, idx: usize) {
        let Some(screen) = self.screen.get_mut(idx) else {
            return;
        };
        screen.needs_redraw = true;
        if !screen.frame_pending {
            self.draw(idx);
        }
    }

    pub fn draw(&mut self, idx: usize) {
        let width = self.screen[idx].width;
        let height = self.screen[idx].height;

        if width == 0 || height == 0 {
            return;
        }

        self.screen[idx].frame_pending = true;

        let stride = width as i32 * 4;

        let (buffer, canvas) = self
            .pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .expect("Failed to create framebuffer");

        let info = ImageInfo::new_n32_premul((width as i32, height as i32), None);
        let mut skia_surface = surfaces::wrap_pixels(&info, canvas, stride as usize, None).unwrap();

        let redraw =
            self.screen[idx]
                .ui
                .draw(skia_surface.canvas(), &self.state, &self.font, width as f32);
        // An animating screen repaints itself from its next frame callback.
        self.screen[idx].needs_redraw = redraw == Redraw::Animating;
        self.update_input_region(idx, width as i32);
        self.sync_catcher(idx);

        let surface = self.screen[idx].layer.wl_surface();
        surface.frame(&self.qh, surface.clone());
        surface.damage_buffer(0, 0, width as i32, height as i32);
        buffer.attach_to(surface).expect("Failed to attach buffer");
        surface.commit();
    }
}

delegate_compositor!(Shell);
delegate_output!(Shell);
delegate_shm!(Shell);

delegate_seat!(Shell);
delegate_pointer!(Shell);

delegate_data_device!(Shell);

delegate_layer!(Shell);

delegate_registry!(Shell);

impl ProvidesRegistryState for Shell {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}
