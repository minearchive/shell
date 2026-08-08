use std::{
    num::NonZeroU32,
    sync::{Arc, RwLock},
};

use calloop::{
    channel::{self, Sender},
    EventLoop, LoopHandle,
};
use calloop_wayland_source::WaylandSource;
use log::{debug, info, warn};
use skia_safe::{surfaces, ImageInfo};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    data_device_manager::{
        data_device::{DataDevice, DataDeviceHandler},
        data_offer::{DataOfferHandler, DragOffer},
        data_source::DataSourceHandler,
        DataDeviceManagerState,
    },
    delegate_compositor, delegate_data_device, delegate_keyboard, delegate_layer, delegate_output,
    delegate_pointer, delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyboardHandler, Keysym},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};

use tokio::sync::mpsc::UnboundedSender;
use wayland_client::{
    globals::registry_queue_init,
    protocol::{
        wl_data_device::WlDataDevice,
        wl_data_device_manager::DndAction,
        wl_keyboard::WlKeyboard,
        wl_output::{Transform, WlOutput},
        wl_pointer::WlPointer,
        wl_seat::WlSeat,
        wl_surface::WlSurface,
    },
    Connection, QueueHandle,
};

use mpris::Event as MprisEvent;

use crate::{
    config::{animation::AnimationConfig, config::Configuration, theme::Theme, WatchableConfig},
    dbus::{
        kdeconnect::{KDEConnectClient, KDEConnectCommand, KDEConnectEvent},
        mpris::{MprisClient, PlayerState},
        notification::{NotificationEvent, NotificationHandle},
        warp::{WarpClient, WarpCommand, WarpStatus},
    },
    ipc::{events::IPCEvent, WindowManagerIPC},
    ui::{Redraw, UiEvent, UserInterface},
};

use ui_core::font::FontBook;

mod components;
mod config;
mod dbus;
mod ipc;

mod ui;

#[allow(unused)]
#[derive(Clone)]
pub struct Commands {
    kdeconnect: UnboundedSender<KDEConnectCommand>,
    warp: UnboundedSender<WarpCommand>,
}

pub struct Screen {
    layer: LayerSurface,
    width: u32,
    height: u32,
    first_configure: bool,
    _keyboard_focus: bool,
    needs_redraw: bool,
    frame_pending: bool,
    ui: UserInterface,
    output: WlOutput,
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
    keyboard: Option<WlKeyboard>,
    data_device: Option<DataDevice>,
    dragging_screen: Option<usize>,
    pointer: Option<WlPointer>,
    screen: Vec<Screen>,
    shm: Shm,
    registry_state: RegistryState,
    ipc: WindowManagerIPC,
    font: FontBook,
    _config: Arc<RwLock<Configuration>>,
    theme: Arc<RwLock<Theme>>,
    animation_config: Arc<RwLock<AnimationConfig>>,
    ui_tx: Sender<UiEvent>,
    commands: Commands,
    exit: bool,
    counter: usize,
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

    let (notification_tx, notification_channel) = channel::channel::<NotificationEvent>();
    let _notification_handle =
        NotificationHandle::init(notification_tx).expect("Failed to start dbus session");
    loop_handle
        .insert_source(notification_channel, |event, _, shell| {
            if let calloop::channel::Event::Msg(events) = event {
                debug!("{events:?}");
                shell.dispatch(|ui| ui.on_notification(events.clone()));
            }
        })
        .unwrap();

    let (mpris_tx, mpris_channel) = channel::channel::<(PlayerState, MprisEvent)>();
    MprisClient::init(mpris_tx);
    loop_handle
        .insert_source(mpris_channel, |event, _, shell| {
            if let calloop::channel::Event::Msg((ref state, ref ev)) = event {
                shell.dispatch(|ui| ui.on_mpris(state, ev));
            }
        })
        .unwrap();

    let (ipc_tx, ipc_channel) = channel::channel::<IPCEvent>();
    let ipc = WindowManagerIPC::new(ipc_tx);
    loop_handle
        .insert_source(ipc_channel, |event, _, shell| {
            if let calloop::channel::Event::Msg(ipc_event) = event {
                shell.dispatch(|ui| ui.on_ipc(ipc_event.clone()));
            }
        })
        .unwrap();

    let (kde_tx, kde_channel) = channel::channel::<KDEConnectEvent>();
    let kde_command_sender = KDEConnectClient::init(kde_tx);
    loop_handle
        .insert_source(kde_channel, |event, _, shell| {
            if let calloop::channel::Event::Msg(kde_event) = event {
                shell.dispatch(|ui| ui.on_kde_connect_event(&kde_event));
            }
        })
        .unwrap();

    let (warp_tx, warp_channel) = channel::channel::<WarpStatus>();
    let warp_command_sender = WarpClient::init(warp_tx);
    loop_handle
        .insert_source(warp_channel, |event, _, shell| {
            if let calloop::channel::Event::Msg(status) = event {
                shell.dispatch(|ui| ui.on_warp(&status));
            }
        })
        .unwrap();

    let (ui_tx, ui_channel) = channel::channel::<UiEvent>();
    loop_handle
        .insert_source(ui_channel, |event, _, shell| {
            if let calloop::channel::Event::Msg(msg) = event {
                match msg {
                    UiEvent::RequestRedraw(idx) => shell.request_redraw(idx),
                    UiEvent::RegisterFont(key, font_family, font_style) => {
                        shell.font.register(key, font_family.as_str(), font_style);
                    }
                    UiEvent::RequestRedrawAll => {
                        for i in 0..shell.counter {
                            shell.request_redraw(i);
                        }
                    }
                    UiEvent::AnimationUpdated(easings) => shell.dispatch(|ui| {
                        easings
                            .iter()
                            .map(|(id, easing)| ui.on_easing_updated(id.clone(), easing.clone()))
                            .max()
                            .unwrap_or_default()
                    }),
                }
            }
        })
        .unwrap();

    let compositor = CompositorState::bind(&globals, &qh).expect("Wayland compositor not found");

    let layer_shell =
        LayerShell::bind(&globals, &qh).expect("Wayland layer shell is not available");

    let data_device_manager_state = DataDeviceManagerState::bind(&globals, &qh)
        .expect("Data Device Manager State is not available");

    let shm = Shm::bind(&globals, &qh).expect("Wayland shared memory is not available");

    let pool = SlotPool::new(256 * 256 * 4, &shm).expect("failed to create pool");

    let config = Configuration::load_and_watch(
        "/home/minearchive/project/gtk_shell/example/config.toml",
        ui_tx.clone(),
    );

    let theme = Theme::load_and_watch(
        "/home/minearchive/project/gtk_shell/example/theme.toml",
        ui_tx.clone(),
    );

    let animation_config = AnimationConfig::load_and_watch(
        "/home/minearchive/project/gtk_shell/example/animation.toml",
        ui_tx.clone(),
    );

    let mut application = Shell {
        pool,
        output_state: OutputState::new(&globals, &qh),
        seat_state: SeatState::new(&globals, &qh),
        compositor_state: compositor,
        data_device_manager_state,
        qh: qh.clone(),
        loop_handle,
        layer_shell,
        keyboard: None,
        pointer: None,
        screen: Vec::new(),
        shm,
        registry_state: RegistryState::new(&globals),
        ipc,
        font: {
            let mut book = FontBook::new();
            book.register(
                "noto_sans",
                "Noto Sans CJK JP",
                skia_safe::FontStyle::normal(),
            );
            book
        },
        _config: config,
        theme,
        animation_config,
        commands: Commands {
            kdeconnect: kde_command_sender,
            warp: warp_command_sender,
        },
        ui_tx,
        exit: false,
        counter: 0,
        data_device: None,
        dragging_screen: None,
    };

    let seats: Vec<_> = application.seat_state.seats().collect();
    for seat in &seats {
        application.ensure_data_device(&qh, seat);
    }

    let signal = event_loop.get_signal();
    event_loop
        .run(None, &mut application, |shell| {
            if shell.exit {
                signal.stop();
            }
        })
        .unwrap();
}

impl CompositorHandler for Shell {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surfaces: &WlSurface, _: u32) {
        if let Some(idx) = self
            .screen
            .iter()
            .position(|p| p.layer.wl_surface() == surfaces)
        {
            self.screen[idx].frame_pending = false;
            if self.screen[idx].needs_redraw {
                self.draw(idx);
            }
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &WlOutput,
    ) {
    }
}

impl OutputHandler for Shell {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
        let surface = self.compositor_state.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Top,
            Some("ApplicationShell"),
            Some(&output),
        );

        layer.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, 60);
        layer.set_exclusive_zone(60);
        layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
        layer.commit();

        let c = self.counter;

        self.screen.push(Screen {
            layer,
            width: 0,
            height: 0,
            first_configure: true,
            _keyboard_focus: false,
            needs_redraw: false,
            frame_pending: false,
            ui: UserInterface::new(
                self.ui_tx.clone(),
                c,
                &mut self.ipc,
                self.commands.clone(),
                Arc::clone(&self.theme),
                Arc::clone(&self.animation_config),
            ),
            output,
        });

        self.counter += 1;
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: WlOutput) {
        self.screen.retain(|s| s.output != output);
    }
}

impl LayerShellHandler for Shell {
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surfaces: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let Some(idx) = self.screen.iter().position(|s| &s.layer == surfaces) else {
            return;
        };

        self.screen[idx].width = NonZeroU32::new(configure.new_size.0).map_or(256, NonZeroU32::get);
        self.screen[idx].height =
            NonZeroU32::new(configure.new_size.1).map_or(256, NonZeroU32::get);

        if self.screen[idx].first_configure {
            self.screen[idx].first_configure = false;
            self.draw(idx);
        }
    }

    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }
}

impl SeatHandler for Shell {
    fn seat_state(&mut self) -> &mut smithay_client_toolkit::seat::SeatState {
        &mut self.seat_state
    }

    fn new_seat(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wayland_client::protocol::wl_seat::WlSeat,
    ) {
        self.ensure_data_device(qh, &seat);
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: smithay_client_toolkit::seat::Capability,
    ) {
        if capability == smithay_client_toolkit::seat::Capability::Keyboard
            && self.keyboard.is_none()
        {
            info!("Set keyboard capability");
            let key = self
                .seat_state
                .get_keyboard(qh, &seat, None)
                .expect("Failed to create keyboard");
            self.keyboard = Some(key);
        }

        if capability == smithay_client_toolkit::seat::Capability::Pointer && self.pointer.is_none()
        {
            info!("Set pointer capability");
            let pointer = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("Failed to create pointer");
            self.pointer = Some(pointer);
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wayland_client::protocol::wl_seat::WlSeat,
        capability: smithay_client_toolkit::seat::Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_some() {
            info!("Unset keyboard capability");
            self.keyboard.take().unwrap().release();
        }

        if capability == Capability::Pointer && self.pointer.is_some() {
            info!("Unset pointer capability");
            self.pointer.take().unwrap().release();
        }
    }

    fn remove_seat(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wayland_client::protocol::wl_seat::WlSeat,
    ) {
    }
}

impl KeyboardHandler for Shell {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: &WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: &WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_keyboard::WlKeyboard,
        _: u32,
        _: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_keyboard::WlKeyboard,
        _: u32,
        _: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_keyboard::WlKeyboard,
        _: u32,
        _: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: smithay_client_toolkit::seat::keyboard::Modifiers,
        _: smithay_client_toolkit::seat::keyboard::RawModifiers,
        _: u32,
    ) {
        info!("Update modifiers: {modifiers:?}");
        for screen in &mut self.screen {
            screen.ui.on_modifier(modifiers);
        }
    }
}

impl PointerHandler for Shell {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _pointer: &wayland_client::protocol::wl_pointer::WlPointer,
        events: &[smithay_client_toolkit::seat::pointer::PointerEvent],
    ) {
        use PointerEventKind::*;
        for event in events {
            let Some(idx) = self
                .screen
                .iter()
                .position(|s| s.layer.wl_surface() == &event.surface)
            else {
                continue;
            };
            self.on_cursor(qh, event, idx);

            match event.kind {
                Enter { .. } => {
                    info!("Pointer entered @{:?}", event.position);
                    let layer = &self.screen[idx].layer;
                    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
                    layer.commit();
                }
                Leave { .. } => {
                    info!("Pointer left");
                }
                Motion { .. } => {}
                Press { button, .. } => {
                    info!("Press {:x} @ {:?}", button, event.position);
                }
                Release { button, .. } => {
                    info!("Release {:x} @ {:?}", button, event.position);
                }
                Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    info!("h: {horizontal:?}, v: {vertical:?}");
                }
            }
        }
    }
}

/// Mime types we know how to log on drop, in preference order.
const PREFERRED_DROP_MIME_TYPES: &[&str] =
    &["text/uri-list", "text/plain;charset=utf-8", "text/plain"];

fn pick_drop_mime(mime_types: &[String]) -> Option<String> {
    PREFERRED_DROP_MIME_TYPES
        .iter()
        .find_map(|preferred| mime_types.iter().find(|m| m.as_str() == *preferred))
        .or_else(|| mime_types.first())
        .cloned()
}

impl DataDeviceHandler for Shell {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
        x: f64,
        y: f64,
        wl_surface: &WlSurface,
    ) {
        info!("DnD enter @{x:.2},{y:.2}");

        let Some(idx) = self
            .screen
            .iter()
            .position(|s| s.layer.wl_surface() == wl_surface)
        else {
            warn!("DnD enter on a surface that belongs to no screen, ignoring");
            return;
        };
        self.dragging_screen = Some(idx);

        let Some(drag_offer) = self.drag_offer() else {
            warn!("DnD enter on screen {idx} without a drag offer");
            return;
        };

        let mime_types = drag_offer.with_mime_types(<[String]>::to_vec);
        if let Some(mime) = pick_drop_mime(&mime_types) {
            drag_offer.accept_mime_type(drag_offer.serial, Some(mime));
        }
        drag_offer.set_actions(DndAction::Copy, DndAction::Copy);

        info!("DnD offer on screen {idx}: {mime_types:?}");
        if self.screen[idx].ui.on_drag_enter(&mime_types) != Redraw::None {
            self.request_redraw(idx);
        }
    }

    fn leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _data_device: &WlDataDevice) {
        if let Some(idx) = self.dragging_screen.take() {
            if self.screen[idx].ui.on_drag_leave() != Redraw::None {
                self.request_redraw(idx);
            }
        }
    }

    fn motion(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
        x: f64,
        y: f64,
    ) {
        if let Some(idx) = self.dragging_screen {
            if self.screen[idx].ui.on_drag_motion(x, y) != Redraw::None {
                self.request_redraw(idx);
            }
        }
    }

    fn selection(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
    ) {
        debug!("Clipboard selection changed (unhandled: this bar is a DnD target only)");
    }

    fn drop_performed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
    ) {
        info!("DnD drop performed");

        let Some(idx) = self.dragging_screen.take() else {
            warn!("Drop performed without a preceding DnD enter, ignoring");
            return;
        };

        let Some(offer) = self.drag_offer() else {
            warn!("Drop on screen {idx} without a drag offer");
            return;
        };

        let mime_types = offer.with_mime_types(<[String]>::to_vec);
        let Some(mime) = pick_drop_mime(&mime_types) else {
            warn!("Drop on screen {idx} offered no mime types");
            offer.finish();
            offer.destroy();
            return;
        };

        let read_pipe = match offer.receive(mime.clone()) {
            Ok(pipe) => pipe,
            Err(err) => {
                warn!("Failed to receive drag-and-drop offer: {err}");
                offer.finish();
                offer.destroy();
                return;
            }
        };

        offer.accept_mime_type(offer.serial, Some(mime.clone()));
        offer.set_actions(DndAction::Copy, DndAction::Copy);

        let mut data = Vec::new();
        let finish_offer = offer.clone();
        let register =
            self.loop_handle
                .clone()
                .insert_source(read_pipe, move |_, f, shell: &mut Shell| {
                    use std::io::BufRead;

                    // SAFETY: the fd stays open (and thus valid) until this closure returns
                    // PostAction::Remove, matching smithay-client-toolkit's own data_device example.
                    let f: &mut std::fs::File = unsafe { f.get_mut() };
                    let mut reader = std::io::BufReader::new(f);
                    match reader.fill_buf() {
                        Ok([]) => {
                            info!("Dropped {} bytes ({mime}) on screen {idx}", data.len());
                            debug!("Dropped data: {:?}", String::from_utf8_lossy(&data));
                            if shell.screen[idx].ui.on_drop(&mime, &data) != Redraw::None {
                                shell.request_redraw(idx);
                            }
                            finish_offer.finish();
                            finish_offer.destroy();
                            calloop::PostAction::Remove
                        }
                        Ok(buf) => {
                            let len = buf.len();
                            data.extend_from_slice(buf);
                            reader.consume(len);
                            calloop::PostAction::Continue
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                            calloop::PostAction::Continue
                        }
                        Err(e) => {
                            warn!("Error reading dropped data: {e}");
                            finish_offer.finish();
                            finish_offer.destroy();
                            calloop::PostAction::Remove
                        }
                    }
                });

        if let Err(err) = register {
            warn!("Failed to register drop read source: {err}");
            offer.finish();
            offer.destroy();
        }
    }
}

impl DataOfferHandler for Shell {
    fn source_actions(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        offer: &mut DragOffer,
        actions: DndAction,
    ) {
        debug!("Drag source advertised actions: {actions:?}");
        offer.set_actions(DndAction::Copy, DndAction::Copy);
    }

    fn selected_action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _offer: &mut DragOffer,
        actions: DndAction,
    ) {
        debug!("Compositor selected drag action: {actions:?}");
    }
}

impl DataSourceHandler for Shell {
    fn accept_mime(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wayland_client::protocol::wl_data_source::WlDataSource,
        _mime: Option<String>,
    ) {
        debug!("DataSourceHandler::accept_mime called, but this bar never creates a drag source");
    }

    fn send_request(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wayland_client::protocol::wl_data_source::WlDataSource,
        _mime: String,
        _fd: smithay_client_toolkit::data_device_manager::WritePipe,
    ) {
        debug!("DataSourceHandler::send_request called, but this bar never creates a drag source");
    }

    fn cancelled(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wayland_client::protocol::wl_data_source::WlDataSource,
    ) {
    }

    fn dnd_dropped(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wayland_client::protocol::wl_data_source::WlDataSource,
    ) {
    }

    fn dnd_finished(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wayland_client::protocol::wl_data_source::WlDataSource,
    ) {
    }

    fn action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wayland_client::protocol::wl_data_source::WlDataSource,
        _action: DndAction,
    ) {
    }
}

impl ShmHandler for Shell {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

pub enum KeyTiming {
    Press,
    Release,
    Repeat,
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

    /// Fan an input event out to every screen's UI and repaint the ones that
    /// asked for it. Collecting the indices first keeps the `&mut self.screen`
    /// borrow from overlapping `request_redraw`.
    fn dispatch(&mut self, mut f: impl FnMut(&mut UserInterface) -> Redraw) {
        let dirty: Vec<usize> = self
            .screen
            .iter_mut()
            .enumerate()
            .filter_map(|(i, s)| (f(&mut s.ui) != Redraw::None).then_some(i))
            .collect();
        for i in dirty {
            self.request_redraw(i);
        }
    }

    /// The drag offer currently being held over this bar, if any.
    fn drag_offer(&self) -> Option<DragOffer> {
        self.data_device
            .as_ref()
            .and_then(|d| d.data().drag_offer())
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

        self.screen[idx].needs_redraw = false;
        self.screen[idx].frame_pending = true;

        let stride = width as i32 * 4;

        let (buffer, canvas) = self
            .pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wayland_client::protocol::wl_shm::Format::Argb8888,
            )
            .expect("Failed to create framebuffer");

        let info = ImageInfo::new_n32_premul((width as i32, height as i32), None);
        let mut skia_surface = surfaces::wrap_pixels(&info, canvas, stride as usize, None).unwrap();

        let font = &self.font;
        self.screen[idx].ui.draw(skia_surface.canvas(), font);

        let surface = self.screen[idx].layer.wl_surface();
        surface.frame(&self.qh, surface.clone());
        surface.damage_buffer(0, 0, width as i32, height as i32);
        buffer.attach_to(surface).expect("Failed to attach buffer");
        surface.commit();
    }

    pub fn on_cursor(&mut self, _qh: &QueueHandle<Self>, event: &PointerEvent, idx: usize) {
        if self.screen[idx].ui.on_cursor(event) != Redraw::None {
            self.request_redraw(idx);
        }
    }
}

delegate_compositor!(Shell);
delegate_output!(Shell);
delegate_shm!(Shell);

delegate_seat!(Shell);
delegate_keyboard!(Shell);
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
