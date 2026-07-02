use std::fmt;

pub type Easing = Box<dyn Fn(f32) -> f32 + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    UnknownFunction(String),
    WrongArgCount { expected: usize, found: usize },
    InvalidNumber(String),
    XOutOfRange(f32),
    Syntax(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnknownFunction(s) => write!(f, "unknown easing function: {s:?}"),
            ParseError::WrongArgCount { expected, found } => {
                write!(f, "expected {expected} arguments, found {found}")
            }
            ParseError::InvalidNumber(s) => write!(f, "invalid number: {s:?}"),
            ParseError::XOutOfRange(x) => {
                write!(f, "cubic-bezier x control point {x} is outside [0, 1]")
            }
            ParseError::Syntax(s) => write!(f, "malformed easing syntax: {s}"),
        }
    }
}

impl std::error::Error for ParseError {}

pub fn css_to_easing(css: &str) -> Result<Easing, ParseError> {
    let css = css.trim();

    match css {
        "ease" => return Ok(cubic_bezier(0.25, 0.1, 0.25, 1.0)),
        "ease-in" => return Ok(cubic_bezier(0.42, 0.0, 1.0, 1.0)),
        "ease-out" => return Ok(cubic_bezier(0.0, 0.0, 0.58, 1.0)),
        "ease-in-out" => return Ok(cubic_bezier(0.42, 0.0, 0.58, 1.0)),
        _ => {}
    }

    let inner = css
        .strip_prefix("cubic-bezier")
        .map(str::trim_start)
        .and_then(|s| s.strip_prefix('('))
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| match css.split(['(', ' ']).next().unwrap_or(css) {
            "cubic-bezier" => ParseError::Syntax(format!("missing parentheses in {css:?}")),
            other => ParseError::UnknownFunction(other.to_string()),
        })?;

    let nums = parse_args(inner)?;
    let [x1, y1, x2, y2] = match nums.as_slice() {
        [a, b, c, d] => [*a, *b, *c, *d],
        _ => {
            return Err(ParseError::WrongArgCount {
                expected: 4,
                found: nums.len(),
            })
        }
    };

    for x in [x1, x2] {
        if !(0.0..=1.0).contains(&x) {
            return Err(ParseError::XOutOfRange(x));
        }
    }

    Ok(cubic_bezier(x1, y1, x2, y2))
}

fn parse_args(inner: &str) -> Result<Vec<f32>, ParseError> {
    inner
        .split(',')
        .map(|arg| {
            let arg = arg.trim();
            arg.parse::<f32>()
                .map_err(|_| ParseError::InvalidNumber(arg.to_string()))
        })
        .collect()
}

pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> Easing {
    let cx = 3.0 * x1;
    let bx = 3.0 * (x2 - x1) - cx;
    let ax = 1.0 - cx - bx;

    let cy = 3.0 * y1;
    let by = 3.0 * (y2 - y1) - cy;
    let ay = 1.0 - cy - by;

    let sample_x = move |s: f32| ((ax * s + bx) * s + cx) * s;
    let sample_y = move |s: f32| ((ay * s + by) * s + cy) * s;
    let sample_dx = move |s: f32| (3.0 * ax * s + 2.0 * bx) * s + cx;

    Box::new(move |t: f32| {
        let t = t.clamp(0.0, 1.0);
        let s = solve_curve_x(t, sample_x, sample_dx);
        sample_y(s)
    })
}

fn solve_curve_x(t: f32, sample_x: impl Fn(f32) -> f32, sample_dx: impl Fn(f32) -> f32) -> f32 {
    const EPSILON: f32 = 1e-6;

    let mut s = t;
    for _ in 0..8 {
        let x = sample_x(s) - t;
        if x.abs() < EPSILON {
            return s;
        }
        let dx = sample_dx(s);
        if dx.abs() < 1e-6 {
            break;
        }
        s -= x / dx;
    }

    let mut lo = 0.0_f32;
    let mut hi = 1.0_f32;
    let mut s = t;
    if s < lo {
        return lo;
    }
    if s > hi {
        return hi;
    }
    while lo < hi {
        let x = sample_x(s);
        if (x - t).abs() < EPSILON {
            return s;
        }
        if t > x {
            lo = s;
        } else {
            hi = s;
        }
        s = (hi - lo) * 0.5 + lo;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn endpoints_are_fixed() {
        let e = cubic_bezier(0.25, 0.1, 0.25, 1.0);
        assert!(approx(e(0.0), 0.0));
        assert!(approx(e(1.0), 1.0));
    }

    #[test]
    fn linear_bezier_is_identity() {
        let e = cubic_bezier(1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0);
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            assert!(approx(e(t), t), "t={t} gave {}", e(t));
        }
    }

    #[test]
    fn ease_is_below_linear_at_start() {
        let e = css_to_easing("ease").unwrap();
        assert!(e(0.5) > 0.5);
    }

    #[test]
    fn parses_explicit_bezier() {
        let e = css_to_easing("cubic-bezier(0.42, 0, 0.58, 1)").unwrap();
        assert!(approx(e(0.0), 0.0));
        assert!(approx(e(1.0), 1.0));
        assert!(approx(e(0.5), 0.5));
    }

    #[test]
    fn tolerates_whitespace() {
        assert!(css_to_easing("  cubic-bezier( 0.1 , 0.2 , 0.3 , 0.4 ) ").is_ok());
    }

    #[test]
    fn rejects_unknown_function() {
        assert_eq!(
            css_to_easing("spring(1, 2)").err(),
            Some(ParseError::UnknownFunction("spring".to_string()))
        );
    }

    #[test]
    fn rejects_wrong_arg_count() {
        assert_eq!(
            css_to_easing("cubic-bezier(0.1, 0.2, 0.3)").err(),
            Some(ParseError::WrongArgCount {
                expected: 4,
                found: 3
            })
        );
    }

    #[test]
    fn rejects_invalid_number() {
        assert!(matches!(
            css_to_easing("cubic-bezier(0.1, foo, 0.3, 0.4)").err(),
            Some(ParseError::InvalidNumber(_))
        ));
    }

    #[test]
    fn rejects_x_out_of_range() {
        assert_eq!(
            css_to_easing("cubic-bezier(1.5, 0, 0.5, 1)").err(),
            Some(ParseError::XOutOfRange(1.5))
        );
    }
}
