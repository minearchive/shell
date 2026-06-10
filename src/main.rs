use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::CssProvider;
use gtk4::DrawingArea;
use gtk4_layer_shell::LayerShell;

fn main() {
    let app = gtk4::Application::builder()
        .application_id("com.example.mybar")
        .build();

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &gtk4::Application) {
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

    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);

    let draw_area = DrawingArea::new();
    draw_area.set_vexpand(true);
    draw_area.set_hexpand(true);
    draw_area.set_height_request(60);

    draw_area.set_draw_func(|_area, context, width, height| {
        context.set_source_rgba(1.0, 1.0, 1.0, 1.0);
        context.rectangle(0.0, height as f64 - 40., width as f64, 40.);
        context.fill().unwrap();
    });

    container.append(&draw_area);

    window.set_child(Some(&container));
    window.present();
}
