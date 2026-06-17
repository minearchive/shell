use std::num::NonZeroU32;

use log::info;
use skia_safe::{surfaces, Color, Color4f, ImageInfo, Paint, Rect};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym},
        pointer::{PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, LayerShell, LayerShellHandler, LayerSurface,
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

mod ipc;

pub struct Application {
    width: u32,
    height: u32,
    first_configure: bool,
    pool: SlotPool,
    output_state: OutputState,
    seat_state: SeatState,
    keyboard: Option<WlKeyboard>,
    pointer: Option<WlPointer>,
    layer: LayerSurface,
    shift: Option<u32>,
    shm: Shm,
    registry_state: RegistryState,
    keyboard_focus: bool,
    exit: bool,
}

fn main() {
    env_logger::init();

    let connection = Connection::connect_to_env().expect("Failed to connect to env");

    let (globals, mut event_queue) = registry_queue_init(&connection).unwrap();
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("Wayland compositor not found");

    let layer_shell =
        LayerShell::bind(&globals, &qh).expect("Wayland layer shell is not available");

    let shm = Shm::bind(&globals, &qh).expect("Wayland shared memory is not available");

    let surface = compositor.create_surface(&qh);

    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        smithay_client_toolkit::shell::wlr_layer::Layer::Top,
        Some("Application Shell"),
        None,
    );

    layer.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);

    layer.set_size(10, 60);
    layer.set_exclusive_zone(60);
    layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);

    layer.commit();

    let pool = SlotPool::new(256 * 256 * 4, &shm).expect("failed to create pool");

    let mut application = Application {
        width: 256,
        height: 256,
        first_configure: true,
        pool,
        output_state: OutputState::new(&globals, &qh),
        seat_state: SeatState::new(&globals, &qh),
        keyboard: None,
        pointer: None,
        layer,
        shift: None,
        shm,
        registry_state: RegistryState::new(&globals),
        keyboard_focus: false,
        exit: false,
    };

    loop {
        event_queue.blocking_dispatch(&mut application).unwrap();

        if application.exit {
            info!("exiting app");
            break;
        }
    }
}

impl CompositorHandler for Application {
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

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: u32) {}

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

impl OutputHandler for Application {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
}

impl LayerShellHandler for Application {
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.width = NonZeroU32::new(configure.new_size.0).map_or(256, NonZeroU32::get);
        self.height = NonZeroU32::new(configure.new_size.1).map_or(256, NonZeroU32::get);

        if self.first_configure {
            self.first_configure = false;
            self.draw(qh);
        }
    }

    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }
}

impl SeatHandler for Application {
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

impl KeyboardHandler for Application {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        surface: &WlSurface,
        _: u32,
        _: &[u32],
        keysyms: &[Keysym],
    ) {
        if self.layer.wl_surface() == surface {
            info!("Keyboard focus on window with pressed symbol: {keysyms:?}");
            self.keyboard_focus = true;
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        surface: &WlSurface,
        _: u32,
    ) {
        if self.layer.wl_surface() == surface {
            info!("Release keyboard focus on window");
            self.keyboard_focus = false;
        }
    }

    fn press_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_keyboard::WlKeyboard,
        _: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.on_key(qh, event, KeyTiming::Press);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_keyboard::WlKeyboard,
        _: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.on_key(qh, event, KeyTiming::Repeat);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_keyboard::WlKeyboard,
        _: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.on_key(qh, event, KeyTiming::Release);
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
        info!("Update modifiers: {modifiers:?}")
    }
}

impl PointerHandler for Application {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wayland_client::protocol::wl_pointer::WlPointer,
        events: &[smithay_client_toolkit::seat::pointer::PointerEvent],
    ) {
        use PointerEventKind::*;
        for event in events {
            // Ignore events for other surfaces
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            match event.kind {
                Enter { .. } => {
                    info!("Pointer entered @{:?}", event.position);
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
                    info!("Scroll H:{horizontal:?}, V:{vertical:?}");
                }
            }
        }
    }
}

impl ShmHandler for Application {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

pub enum KeyTiming {
    Press,
    Release,
    Repeat,
}

impl Application {
    pub fn draw(&mut self, qh: &QueueHandle<Self>) {
        let width = self.width;
        let height = self.height;
        let stride = self.width as i32 * 4;

        let (buffer, canvas) = self
            .pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wayland_client::protocol::wl_shm::Format::Argb8888,
            )
            .expect("Failed to create framebuffer");

        {
            let info = ImageInfo::new_n32_premul((width as i32, height as i32), None);
            let mut skia_surface =
                surfaces::wrap_pixels(&info, canvas, stride as usize, None).unwrap();

            let canvas = skia_surface.canvas();
            let mut paint = Paint::default();

            paint.set_anti_alias(true);
            paint.set_color(Color::from_argb(255, 255, 255, 255));

            canvas.clear(Color4f::new(0., 0., 0., 0.));
            canvas.draw_rect(Rect::from_xywh(0., 0., width as f32, height as f32), &paint);
        }

        self.layer
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);

        self.layer
            .wl_surface()
            .frame(qh, self.layer.wl_surface().clone());

        buffer
            .attach_to(self.layer.wl_surface())
            .expect("Failed to attach buffer");

        self.layer.commit();
    }

    pub fn on_key(&mut self, _qh: &QueueHandle<Self>, _event: KeyEvent, _timing: KeyTiming) {}
}

delegate_compositor!(Application);
delegate_output!(Application);
delegate_shm!(Application);

delegate_seat!(Application);
delegate_keyboard!(Application);
delegate_pointer!(Application);

delegate_layer!(Application);

delegate_registry!(Application);

impl ProvidesRegistryState for Application {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}
