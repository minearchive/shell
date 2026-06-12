use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::CssProvider;
use gtk4::GLArea;
use gtk4_layer_shell::LayerShell;
use skia_safe::gpu::DirectContext;
use skia_safe::Canvas;
use skia_safe::Color;
use skia_safe::Color4f;
use skia_safe::Paint;
use skia_safe::Rect;
use std::cell::RefCell;
use std::rc::Rc;

mod ipc;
mod util;

pub struct BarState {
    pub gl_area: GLArea,
    pub skia_context: Option<DirectContext>,
}

#[derive(Default)]
pub struct ShellState {
    pub wm: ipc::WindowManager,
    pub bars: Vec<BarState>,
}

fn main() {
    let app = gtk4::Application::builder()
        .application_id("com.example.mybar")
        .build();

    let state = Rc::new(RefCell::new(ShellState::default()));

    app.connect_activate(move |app| build_ui(app, Rc::clone(&state)));
    app.run();
}

fn build_ui(app: &gtk4::Application, state: Rc<RefCell<ShellState>>) {
    let display = gdk::Display::default().expect("Could not get default display");

    let monitors = display.monitors();
    let n_monitors = monitors.n_items();

    for i in 0..n_monitors {
        if let Some(monitor) = monitors
            .item(i)
            .and_then(|item| item.downcast::<gdk::Monitor>().ok())
        {
            create_bar_for_monitor(app, &monitor, Rc::clone(&state));
        }
    }
}

fn create_bar_for_monitor(
    app: &gtk4::Application,
    monitor: &gdk::Monitor,
    state: Rc<RefCell<ShellState>>,
) {
    let css_provider = CssProvider::new();
    css_provider.load_from_data("window { background: transparent; }");

    let window = gtk4::ApplicationWindow::new(app);

    gtk4::style_context_add_provider_for_display(
        &WidgetExt::display(&window),
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    window.init_layer_shell();
    window.set_height_request(60);
    window.set_exclusive_zone(40);
    window.set_layer(gtk4_layer_shell::Layer::Top);
    window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
    window.set_anchor(gtk4_layer_shell::Edge::Left, true);
    window.set_anchor(gtk4_layer_shell::Edge::Right, true);

    window.set_monitor(monitor);

    let gl_area = GLArea::new();
    gl_area.set_has_depth_buffer(false);
    gl_area.set_has_stencil_buffer(true);
    gl_area.set_auto_render(false);
    gl_area.set_hexpand(true);
    gl_area.set_vexpand(true);

    let bar_index = {
        let mut s = state.borrow_mut();
        s.bars.push(BarState {
            gl_area: gl_area.clone(),
            skia_context: None,
        });
        s.bars.len() - 1
    };

    let state_realize = Rc::clone(&state);

    gl_area.connect_realize(move |area| {
        area.make_current();

        if let Some(err) = area.error() {
            eprintln!("GLArea realize error: {}", err);
            return;
        }

        let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }

            epoxy::get_proc_addr(name) as *const _
        })
        .expect("Failed to create skia gl interface");

        let gr_context = skia_safe::gpu::direct_contexts::make_gl(interface, None)
            .expect("Failed to create direct context");

        state_realize.borrow_mut().bars[bar_index].skia_context = Some(gr_context);
    });

    gl_area.connect_render(move |area, _context| {
        let mut fboid: i32 = 0;

        unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

        let fb_info = skia_safe::gpu::gl::FramebufferInfo {
            fboid: fboid as u32,
            format: skia_safe::gpu::gl::Format::RGBA8.into(),
            ..Default::default()
        };

        let (w, h) = (area.width(), area.height());
        let target = skia_safe::gpu::backend_render_targets::make_gl(
            (w, h),
            0, // sample
            8, // stencil bits
            fb_info,
        );

        let mut surface = skia_safe::gpu::surfaces::wrap_backend_render_target(
            state.borrow_mut().bars[bar_index]
                .skia_context
                .as_mut()
                .unwrap(),
            &target,
            skia_safe::gpu::SurfaceOrigin::BottomLeft,
            skia_safe::ColorType::RGBA8888,
            None,
            None,
        )
        .expect("Failed to create surface");

        draw_in_canvas(area, surface.canvas(), Rc::clone(&state));

        gtk4::glib::Propagation::Stop
    });

    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    container.append(&gl_area);

    window.set_child(Some(&container));
    window.present();
}

pub fn draw_in_canvas(area: &GLArea, canvas: &Canvas, shell_state: Rc<RefCell<ShellState>>) {
    canvas.clear(Color4f::new(0., 0., 0., 0.));

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgb(255, 0, 0));
    paint.set_anti_alias(true);
    canvas.draw_rect(
        Rect::from_xywh(0., 0., area.width() as f32, area.height() as f32),
        &paint,
    );
}
