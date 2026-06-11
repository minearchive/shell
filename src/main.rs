use gtk4::cairo::Operator;
use gtk4::ffi::GtkGLArea;
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::CssProvider;
use gtk4::DrawingArea;
use gtk4_layer_shell::LayerShell;
use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::util::rounded_rectangle;
use crate::util::Corners;

mod ipc;
mod util;

#[derive(Default)]
pub struct ShellState {
    pub wm: ipc::WindowManager,
}

fn main() {
    let app = gtk4::Application::builder()
        .application_id("com.example.mybar")
        .build();

    let state = Rc::new(RefCell::new(ShellState {
        wm: ipc::WindowManager::Unknown,
    }));

    app.connect_activate(move |app| build_ui(app, &state.borrow()));
    app.run();
}

fn build_ui(app: &gtk4::Application, state: &ShellState) {
    let display = gdk::Display::default().expect("Could not get default display");

    let monitors = display.monitors();
    let n_monitors = monitors.n_items();

    for i in 0..n_monitors {
        if let Some(monitor) = monitors
            .item(i)
            .and_then(|item| item.downcast::<gdk::Monitor>().ok())
        {
            create_bar_for_monitor(app, &monitor);
        }
    }
}

fn create_bar_for_monitor(app: &gtk4::Application, monitor: &gdk::Monitor) {
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

    let draw_area = DrawingArea::new();
    draw_area.set_hexpand(true);
    draw_area.set_vexpand(true);
    draw_area.set_draw_func(|_, context, width, height| {
        context.set_source_rgba(0.1, 0.1, 0.1, 0.85);
        context.translate(0., 20.);
        rounded_rectangle(
            context,
            0.0,
            0.0,
            width as f64,
            height as f64,
            Corners::uniform(8.0),
        );
        let _ = context.fill();
    });

    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);

    container.append(&draw_area);

    window.set_child(Some(&container));
    window.present();
}
