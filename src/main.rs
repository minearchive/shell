use std::{
    num::NonZeroU32,
    sync::{Arc, RwLock},
};

use calloop::{
    channel::{self, Sender},
    EventLoop, LoopHandle,
};
use calloop_wayland_source::WaylandSource;
use log::{debug, info};
use skia_safe::{surfaces, ImageInfo};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
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

use wayland_client::{
    globals::registry_queue_init,
    protocol::{
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
    config::{animation::AnimationConfig, config::Configuration, WatchableConfig},
    dbus::{
        kdeconnect::{KDEConnectClient, KDEConnectEvent},
        mpris::{MprisClient, PlayerState},
        notification::{NotificationEvent, NotificationHandle},
    },
    font::FontBook,
    ipc::{events::IPCEvent, WindowManagerIPC},
    ui::{UiEvent, UserInterface},
};

mod animation;
mod components;
mod config;
mod dbus;
mod font;
mod ipc;

mod ui;
mod util;

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
    layer_shell: LayerShell,
    qh: QueueHandle<Shell>,
    _loop_handle: LoopHandle<'static, Shell>,
    keyboard: Option<WlKeyboard>,
    pointer: Option<WlPointer>,
    screen: Vec<Screen>,
    shift: Option<u32>,
    shm: Shm,
    registry_state: RegistryState,
    ipc: WindowManagerIPC,
    font: FontBook,
    config: Arc<RwLock<Configuration>>,
    animation_config: Arc<RwLock<AnimationConfig>>,
    ui_tx: Sender<UiEvent>,
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
    // Bind the handle for the whole lifetime of `main`: dropping it would close
    // the zbus `Connection`, releasing the `org.freedesktop.Notifications` name
    // and tearing down the object-server task.
    let _notification_handle =
        NotificationHandle::init(notification_tx).expect("Failed to start dbus session");
    loop_handle
        .insert_source(notification_channel, |event, _, shell| {
            if let calloop::channel::Event::Msg(events) = event {
                debug!("{events:?}");
                for screen in &mut shell.screen {
                    screen.ui.on_notification(events.clone());
                }
            }
        })
        .unwrap();

    let (mpris_tx, mpris_channel) = channel::channel::<(PlayerState, MprisEvent)>();
    MprisClient::init(mpris_tx);
    loop_handle
        .insert_source(mpris_channel, |event, _, shell| {
            if let calloop::channel::Event::Msg((ref state, ref ev)) = event {
                for screen in &mut shell.screen {
                    screen.ui.on_mpris(state, ev);
                }
            }
        })
        .unwrap();

    let (ipc_tx, ipc_channel) = channel::channel::<IPCEvent>();
    let ipc = WindowManagerIPC::new(ipc_tx);
    loop_handle
        .insert_source(ipc_channel, |event, _, shell| {
            if let calloop::channel::Event::Msg(ipc_event) = event {
                for screen in &mut shell.screen {
                    screen.ui.on_ipc(ipc_event.clone());
                }
            }
        })
        .unwrap();

    let (kde_tx, kde_channel) = channel::channel::<KDEConnectEvent>();
    let _kde_command_sender = KDEConnectClient::init(kde_tx);
    loop_handle
        .insert_source(kde_channel, |event, _, shell| {
            if let calloop::channel::Event::Msg(kde_event) = event {
                for screen in &mut shell.screen {
                    screen.ui.on_kde_connect_event(&kde_event.clone());
                }
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
                    UiEvent::AnimationUpdated(easings) => {
                        for screen in &mut shell.screen {
                            for (id, easing) in &easings {
                                screen.ui.on_easing_updated(id.clone(), easing.clone());
                            }
                        }
                    }
                }
            }
        })
        .unwrap();

    let compositor = CompositorState::bind(&globals, &qh).expect("Wayland compositor not found");

    let layer_shell =
        LayerShell::bind(&globals, &qh).expect("Wayland layer shell is not available");

    let shm = Shm::bind(&globals, &qh).expect("Wayland shared memory is not available");

    let pool = SlotPool::new(256 * 256 * 4, &shm).expect("failed to create pool");

    let config = Configuration::load_and_watch(
        "/home/minearchive/project/gtk_shell/example/config.toml",
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
        qh: qh.clone(),
        _loop_handle: loop_handle,
        layer_shell,
        keyboard: None,
        pointer: None,
        screen: Vec::new(),
        shift: None,
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
        config,
        animation_config,
        ui_tx,
        exit: false,
        counter: 0,
    };

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
                Arc::clone(&self.config),
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
        _: &QueueHandle<Self>,
        _: wayland_client::protocol::wl_seat::WlSeat,
    ) {
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
        // if let Some(find) = self
        //     .screen
        //     .iter_mut()
        //     .find(|s| s.layer.wl_surface() == surface)
        // {
        //     info!("Keyboard focus on window with pressed symbol: {keysyms:?}");
        //     find.keyboard_focus = true;
        // };
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: &WlSurface,
        _: u32,
    ) {
        // if let Some(find) = self
        //     .screen
        //     .iter_mut()
        //     .find(|s| s.layer.wl_surface() == surface)
        // {
        //     info!("Release keyboard focus on window");
        //     find.keyboard_focus = false;
        // };
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_keyboard::WlKeyboard,
        _: u32,
        _: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        // self.on_key(qh, event, KeyTiming::Press);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_keyboard::WlKeyboard,
        _: u32,
        _: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        // self.on_key(qh, event, KeyTiming::Repeat);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_keyboard::WlKeyboard,
        _: u32,
        _: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        // self.on_key(qh, event, KeyTiming::Release);
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
            if let Some(idx) = self
                .screen
                .iter()
                .position(|s| s.layer.wl_surface() == &event.surface)
            {
                self.on_cursor(qh, event, idx);
            }

            if !self
                .screen
                .iter()
                .any(|s| s.layer.wl_surface() == &event.surface)
            {
                continue;
            }

            {
                match event.kind {
                    Enter { .. } => {
                        info!("Pointer entered @{:?}", event.position);
                        if let Some(screen) = self
                            .screen
                            .iter()
                            .find(|s| s.layer.wl_surface() == &event.surface)
                        {
                            screen
                                .layer
                                .set_keyboard_interactivity(KeyboardInteractivity::None);
                            screen.layer.commit();
                        }
                    }
                    Leave { .. } => {
                        info!("Pointer left");
                    }
                    Motion { .. } => {}
                    Press { button, .. } => {
                        info!("Press {:x} @ {:?}", button, event.position);
                        self.shift = self.shift.xor(Some(0));
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

    // pub fn on_key(&mut self, _qh: &QueueHandle<Self>, event: KeyEvent, timing: KeyTiming) {
    //     for screen in &mut self.screen {
    //         screen.ui.on_key(&event, &timing);
    //     }
    // }

    pub fn on_cursor(&mut self, _qh: &QueueHandle<Self>, event: &PointerEvent, idx: usize) {
        self.screen[idx].ui.on_cursor(event);
    }
}

delegate_compositor!(Shell);
delegate_output!(Shell);
delegate_shm!(Shell);

delegate_seat!(Shell);
delegate_keyboard!(Shell);
delegate_pointer!(Shell);

delegate_layer!(Shell);

delegate_registry!(Shell);

impl ProvidesRegistryState for Shell {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}
