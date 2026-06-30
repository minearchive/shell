use std::time::{Duration, Instant};

pub struct Animation<T> {
    begin: T,
    end: T,
    started: Instant,
    duration: Duration,
    easing: fn(f32) -> f32,
}

impl Animation<f32> {
    pub fn new(begin: f32, end: f32, duration: Duration, easing: fn(f32) -> f32) -> Self {
        Self {
            begin,
            end,
            started: Instant::now(),
            duration,
            easing,
        }
    }

    pub fn value(&self) -> f32 {
        if self.is_done() {
            return self.end();
        }
        let t = self.started.elapsed().as_secs_f32() / self.duration.as_secs_f32().min(1.0);
        let t_eased = (self.easing)(t);
        self.begin + (self.end - self.begin) * t_eased
    }

    pub fn reverse(&mut self) {
        self.reset(self.end(), self.begin());
    }

    pub fn is_done(&self) -> bool {
        self.started.elapsed() >= self.duration
    }

    pub fn reset(&mut self, begin: f32, end: f32) {
        self.begin = begin;
        self.end = end;
        self.started = Instant::now();
    }

    pub fn set_easing(&mut self, easing: fn(f32) -> f32) {
        self.easing = easing;
    }

    pub fn set_duration(&mut self, duration: Duration) {
        self.duration = duration
    }

    pub fn begin(&self) -> f32 {
        self.begin
    }
    pub fn end(&self) -> f32 {
        self.end
    }
}

#[allow(unused)]
pub mod easing {
    use std::f32::consts::PI;

    pub fn linear(t: f32) -> f32 {
        t
    }

    pub fn ease_in_sine(t: f32) -> f32 {
        1.0 - ((t * PI) / 2.0).cos()
    }

    pub fn ease_out_sine(t: f32) -> f32 {
        ((t * PI) / 2.0).sin()
    }

    pub fn ease_in_out_sine(t: f32) -> f32 {
        -((PI * t).cos() - 1.0) / 2.0
    }

    pub fn ease_in_quad(t: f32) -> f32 {
        t * t
    }

    pub fn ease_out_quad(t: f32) -> f32 {
        1.0 - (1.0 - t) * (1.0 - t)
    }

    pub fn ease_in_out_quad(t: f32) -> f32 {
        if t < 0.5 {
            2.0 * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
        }
    }

    pub fn ease_in_cubic(t: f32) -> f32 {
        t * t * t
    }

    pub fn ease_out_cubic(t: f32) -> f32 {
        1.0 - (1.0 - t).powi(3)
    }

    pub fn ease_in_out_cubic(t: f32) -> f32 {
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        }
    }

    pub fn ease_in_quart(t: f32) -> f32 {
        t * t * t * t
    }

    pub fn ease_out_quart(t: f32) -> f32 {
        1.0 - (1.0 - t).powi(4)
    }

    pub fn ease_in_out_quart(t: f32) -> f32 {
        if t < 0.5 {
            8.0 * t * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
        }
    }

    pub fn ease_in_quint(t: f32) -> f32 {
        t * t * t * t * t
    }

    pub fn ease_out_quint(t: f32) -> f32 {
        1.0 - (1.0 - t).powi(5)
    }

    pub fn ease_in_out_quint(t: f32) -> f32 {
        if t < 0.5 {
            16.0 * t * t * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(5) / 2.0
        }
    }

    pub fn ease_in_expo(t: f32) -> f32 {
        if t == 0.0 {
            0.0
        } else {
            2.0_f32.powf(10.0 * t - 10.0)
        }
    }

    pub fn ease_out_expo(t: f32) -> f32 {
        if t == 1.0 {
            1.0
        } else {
            1.0 - 2.0_f32.powf(-10.0 * t)
        }
    }

    pub fn ease_in_out_expo(t: f32) -> f32 {
        if t == 0.0 {
            return 0.0;
        }
        if t == 1.0 {
            return 1.0;
        }
        if t < 0.5 {
            2.0_f32.powf(20.0 * t - 10.0) / 2.0
        } else {
            (2.0 - 2.0_f32.powf(-20.0 * t + 10.0)) / 2.0
        }
    }

    pub fn ease_in_circ(t: f32) -> f32 {
        1.0 - (1.0 - t * t).sqrt()
    }

    pub fn ease_out_circ(t: f32) -> f32 {
        (1.0 - (t - 1.0) * (t - 1.0)).sqrt()
    }

    pub fn ease_in_out_circ(t: f32) -> f32 {
        if t < 0.5 {
            (1.0 - (1.0 - (2.0 * t).powi(2)).sqrt()) / 2.0
        } else {
            ((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt() + 1.0) / 2.0
        }
    }

    pub fn ease_in_back(t: f32) -> f32 {
        const C1: f32 = 1.70158;
        const C3: f32 = C1 + 1.0;
        C3 * t * t * t - C1 * t * t
    }

    pub fn ease_out_back(t: f32) -> f32 {
        const C1: f32 = 1.70158;
        const C3: f32 = C1 + 1.0;
        1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
    }

    pub fn ease_in_out_back(t: f32) -> f32 {
        const C2: f32 = 2.5949095;
        if t < 0.5 {
            ((2.0 * t).powi(2) * ((C2 + 1.0) * 2.0 * t - C2)) / 2.0
        } else {
            ((2.0 * t - 2.0).powi(2) * ((C2 + 1.0) * (t * 2.0 - 2.0) + C2) + 2.0) / 2.0
        }
    }

    pub fn ease_in_elastic(t: f32) -> f32 {
        if t == 0.0 {
            return 0.0;
        }
        if t == 1.0 {
            return 1.0;
        }
        const C4: f32 = (2.0 * PI) / 3.0;
        -(2.0_f32.powf(10.0 * t - 10.0) * ((t * 10.0 - 10.75) * C4).sin())
    }

    pub fn ease_out_elastic(t: f32) -> f32 {
        if t == 0.0 {
            return 0.0;
        }
        if t == 1.0 {
            return 1.0;
        }
        const C4: f32 = (2.0 * PI) / 3.0;
        2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * C4).sin() + 1.0
    }

    pub fn ease_in_out_elastic(t: f32) -> f32 {
        if t == 0.0 {
            return 0.0;
        }
        if t == 1.0 {
            return 1.0;
        }
        const C5: f32 = (2.0 * PI) / 4.5;
        if t < 0.5 {
            -(2.0_f32.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * C5).sin()) / 2.0
        } else {
            2.0_f32.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * C5).sin() / 2.0 + 1.0
        }
    }

    pub fn ease_out_bounce(mut t: f32) -> f32 {
        const N1: f32 = 7.5625;
        const D1: f32 = 2.75;
        if t < 1.0 / D1 {
            N1 * t * t
        } else if t < 2.0 / D1 {
            t -= 1.5 / D1;
            N1 * t * t + 0.75
        } else if t < 2.5 / D1 {
            t -= 2.25 / D1;
            N1 * t * t + 0.9375
        } else {
            t -= 2.625 / D1;
            N1 * t * t + 0.984375
        }
    }

    pub fn ease_in_bounce(t: f32) -> f32 {
        1.0 - ease_out_bounce(1.0 - t)
    }

    pub fn ease_in_out_bounce(t: f32) -> f32 {
        if t < 0.5 {
            (1.0 - ease_out_bounce(1.0 - 2.0 * t)) / 2.0
        } else {
            (1.0 + ease_out_bounce(2.0 * t - 1.0)) / 2.0
        }
    }
}
