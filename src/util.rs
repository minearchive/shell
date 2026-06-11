use gtk4::cairo::Context;
use std::f64::consts::PI;

pub struct Corners {
    pub top_left: f64,
    pub top_right: f64,
    pub bottom_right: f64,
    pub bottom_left: f64,
}

impl Corners {
    pub fn uniform(r: f64) -> Self {
        Self {
            top_left: r,
            top_right: r,
            bottom_right: r,
            bottom_left: r,
        }
    }
}

pub fn rounded_rectangle(
    context: &Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    corners: Corners,
) {
    context.new_sub_path();
    context.arc(
        x + width - corners.top_right,
        y + corners.top_right,
        corners.top_right,
        -PI / 2.0,
        0.0,
    );
    context.arc(
        x + width - corners.bottom_right,
        y + height - corners.bottom_right,
        corners.bottom_right,
        0.0,
        PI / 2.0,
    );
    context.arc(
        x + corners.bottom_left,
        y + height - corners.bottom_left,
        corners.bottom_left,
        PI / 2.0,
        PI,
    );
    context.arc(
        x + corners.top_left,
        y + corners.top_left,
        corners.top_left,
        PI,
        3.0 * PI / 2.0,
    );
    context.close_path();
}
