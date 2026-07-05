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
        "linear" => return Ok(Box::new(|t: f32| t)),
        "ease" => return Ok(cubic_bezier(0.25, 0.1, 0.25, 1.0)),
        "ease-in" => return Ok(cubic_bezier(0.42, 0.0, 1.0, 1.0)),
        "ease-out" => return Ok(cubic_bezier(0.0, 0.0, 0.58, 1.0)),
        "ease-in-out" => return Ok(cubic_bezier(0.42, 0.0, 0.58, 1.0)),
        _ => {}
    }

    if let Some(rest) = css.strip_prefix("linear").map(str::trim_start) {
        if rest.starts_with('(') {
            let inner = rest
                .strip_prefix('(')
                .and_then(|s| s.strip_suffix(')'))
                .ok_or_else(|| ParseError::Syntax(format!("missing parentheses in {css:?}")))?;
            let stops = parse_linear_stops(inner)?;
            return Ok(linear_stops(stops));
        }
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

fn parse_percentage(s: &str) -> Result<f32, ParseError> {
    let digits = s
        .strip_suffix('%')
        .ok_or_else(|| ParseError::Syntax(format!("expected percentage, found {s:?}")))?;
    digits
        .parse::<f32>()
        .map(|v| v / 100.0)
        .map_err(|_| ParseError::InvalidNumber(s.to_string()))
}

fn parse_linear_stops(inner: &str) -> Result<Vec<(f32, f32)>, ParseError> {
    let mut raw: Vec<(f32, Option<f32>)> = Vec::new();

    for token in inner.split(',') {
        let token = token.trim();
        if token.is_empty() {
            return Err(ParseError::Syntax("empty stop in linear()".to_string()));
        }
        let mut parts = token.split_whitespace();

        let val_str = parts.next().unwrap();
        let val = val_str
            .parse::<f32>()
            .map_err(|_| ParseError::InvalidNumber(val_str.to_string()))?;

        let pos1 = parts.next().map(parse_percentage).transpose()?;
        let pos2 = parts.next().map(parse_percentage).transpose()?;

        if parts.next().is_some() {
            return Err(ParseError::Syntax(format!(
                "too many values in stop: {token:?}"
            )));
        }

        raw.push((val, pos1));
        if let Some(p) = pos2 {
            raw.push((val, Some(p)));
        }
    }

    if raw.is_empty() {
        return Err(ParseError::Syntax(
            "linear() requires at least one stop".to_string(),
        ));
    }

    Ok(resolve_positions(raw))
}

fn resolve_positions(mut stops: Vec<(f32, Option<f32>)>) -> Vec<(f32, f32)> {
    let n = stops.len();
    if n == 0 {
        return vec![];
    }

    if stops[0].1.is_none() {
        stops[0].1 = Some(0.0);
    }
    if stops[n - 1].1.is_none() {
        stops[n - 1].1 = Some(1.0);
    }

    // Distribute positions evenly within each unpositioned run.
    let mut i = 1;
    while i < n - 1 {
        if stops[i].1.is_none() {
            let start = i - 1;
            let mut end = i + 1;
            while end < n && stops[end].1.is_none() {
                end += 1;
            }
            let p0 = stops[start].1.unwrap();
            let p1 = stops[end].1.unwrap();
            let span = (end - start) as f32;
            for k in start + 1..end {
                stops[k].1 = Some(p0 + (k - start) as f32 / span * (p1 - p0));
            }
            i = end;
        } else {
            i += 1;
        }
    }

    stops.into_iter().map(|(v, p)| (v, p.unwrap())).collect()
}

pub fn linear_stops(stops: Vec<(f32, f32)>) -> Easing {
    Box::new(move |t: f32| {
        if stops.is_empty() {
            return t;
        }
        if stops.len() == 1 {
            return stops[0].0;
        }

        let t = t.clamp(0.0, 1.0);

        if t <= stops[0].1 {
            return stops[0].0;
        }
        if t >= stops[stops.len() - 1].1 {
            return stops[stops.len() - 1].0;
        }

        // partition_point guarantees p0 <= t < p1, so p1 - p0 > 0.
        let idx = stops.partition_point(|&(_, p)| p <= t);
        let (v0, p0) = stops[idx - 1];
        let (v1, p1) = stops[idx];
        v0 + (t - p0) / (p1 - p0) * (v1 - v0)
    })
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
        if dx.abs() < EPSILON {
            break;
        }
        s -= x / dx;
    }

    // Bisection fallback: x(s) is monotonic on [0, 1] since x1, x2 are in [0, 1].
    let mut lo = 0.0_f32;
    let mut hi = 1.0_f32;
    while hi - lo > EPSILON {
        let mid = (lo + hi) * 0.5;
        if sample_x(mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) * 0.5
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
    fn linear_is_identity() {
        let e = css_to_easing("linear").unwrap();
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            assert!(approx(e(t), t), "t={t} gave {}", e(t));
        }
    }

    #[test]
    fn linear_stops_evenly_spaced() {
        // three stops, no explicit positions → 0%, 50%, 100%
        let e = css_to_easing("linear(0, 0.5, 1)").unwrap();
        assert!(approx(e(0.0), 0.0));
        assert!(approx(e(0.5), 0.5));
        assert!(approx(e(1.0), 1.0));
        assert!(approx(e(0.25), 0.25));
    }

    #[test]
    fn linear_stops_explicit_positions() {
        // output jumps from 0 to 1 at 50%, flat thereafter
        let e = css_to_easing("linear(0 0%, 1 50%)").unwrap();
        assert!(approx(e(0.0), 0.0));
        assert!(approx(e(0.25), 0.5));
        assert!(approx(e(0.5), 1.0));
        assert!(approx(e(1.0), 1.0));
    }

    #[test]
    fn linear_stops_two_position_stop() {
        // "1 25% 75%" expands to two stops: (1, 25%) and (1, 75%) → flat middle
        let e = css_to_easing("linear(0, 1 25% 75%, 0)").unwrap();
        assert!(approx(e(0.0), 0.0));
        assert!(approx(e(0.5), 1.0));
        assert!(approx(e(1.0), 0.0));
    }

    #[test]
    fn linear_stops_spring_like() {
        // smoke-test the large stop list from the CSS spring preset
        let e = css_to_easing(
            "linear(0, 0.0146, 0.0548, 0.1153, 0.1911, 0.2776, 0.3706, 0.4665, \
             0.562, 0.6545, 0.7419, 0.8225, 0.8951, 0.9589, 1.0137, 1.0591, \
             1.0956, 1.1235, 1.1434, 1.156, 1.1621, 1.1626, 1.1585, 1.1505, \
             1.1396, 1.1264, 1.1118, 1.0964, 1.0807, 1.0653, 1.0505, 1.0367, \
             1.0241, 1.0128, 1.003, 0.9946, 0.9878, 0.9824, 0.9784, 0.9757, \
             0.9741, 0.9734, 0.9737, 0.9746, 0.9761, 0.9781, 0.9803, 0.9828, \
             0.9853, 0.9878, 0.9903, 0.9927, 0.9949, 0.9968, 0.9986, 1.0001, \
             1.0013, 1.0024, 1.0032, 1.0037, 1.0041, 1.0043, 1.0043, 1.0042, \
             1.0041, 1.0038, 1.0034, 1.0031, 1.0026, 1.0022, 1.0018, 1.0014, \
             1.001, 1.0007, 1.0004, 1.0001, 0.9999, 0.9997, 0.9996, 0.9994, \
             0.9994, 0.9993, 0.9993, 0.9993, 0.9993, 0.9994, 0.9994, 0.9995, \
             0.9995, 0.9996, 0.9997, 0.9997, 0.9998, 0.9999, 0.9999, 1, 1, 1, \
             1.0001, 1.0001, 1)",
        )
        .unwrap();
        assert!(approx(e(0.0), 0.0));
        assert!(approx(e(1.0), 1.0));
        // overshoots near the start
        assert!(e(0.2) > 1.0);
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
