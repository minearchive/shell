use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;

fn main() {
    let app = gtk4::Application::builder()
        .application_id("com.example.mybar")
        .build();

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &gtk4::Application) {
    // Get the display
    let display = gdk::Display::default().expect("Could not get default display");

    // Get all monitors
    let monitors = display.monitors();
    let n_monitors = monitors.n_items();

    // Create a window for each monitor
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
    let window = gtk4::ApplicationWindow::new(app);

    // レイヤーシェル設定
    window.init_layer_shell();
    window.set_height_request(30);
    window.set_exclusive_zone(30);
    window.set_layer(gtk4_layer_shell::Layer::Top);
    window.set_anchor(gtk4_layer_shell::Edge::Top, true);
    window.set_anchor(gtk4_layer_shell::Edge::Left, true);
    window.set_anchor(gtk4_layer_shell::Edge::Right, true);

    // Set the monitor for this window
    window.set_monitor(monitor);

    // バーの内容を構築
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    container.set_margin_start(10);
    container.set_margin_end(10);

    // Display monitor information
    let monitor_info = format!(
        "My Bar - Monitor: {}",
        monitor.model().unwrap_or_else(|| "Unknown".into())
    );
    let label = gtk4::Label::new(Some(&monitor_info));
    container.append(&label);

    window.set_child(Some(&container));
    window.present();
}
