pub mod clock;
pub mod media;
pub mod notifications;
pub mod toolbox;

use m3_widget::Slider;
use ui_core::animation::parser::Easing;

/// Applies the `slider_handle` and `slider_indicator` easings to any slider in the bar.
pub fn update_slider_easing(slider: &mut Slider, id: &str, easing: &Easing) {
    match id {
        "slider_handle" => slider.set_handle_easing(easing.clone()),
        "slider_indicator" => slider.set_indicator_easing(easing.clone()),
        _ => {}
    }
}
