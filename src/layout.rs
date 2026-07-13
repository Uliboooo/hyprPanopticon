//! Pure circular-layout math. No GTK types so it stays unit-testable.

use std::f64::consts::PI;

#[derive(Debug, Clone, Copy)]
pub struct RingParams {
    /// Scale of the focused preview.
    pub s_max: f64,
    /// Scale of the preview diametrically opposite the focus.
    pub s_min: f64,
    /// Falloff sharpness exponent.
    pub falloff: f64,
    /// Focused preview width as a fraction of screen width.
    pub focus_width_frac: f64,
    /// Margin kept between previews and the screen edge, in px.
    pub margin: f64,
}

impl Default for RingParams {
    fn default() -> Self {
        Self {
            s_max: 1.0,
            s_min: 0.35,
            falloff: 2.0,
            focus_width_frac: 0.34,
            margin: 24.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Top-left corner.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Scale relative to the focused preview; also usable as z-order key.
    pub scale: f64,
}

/// Wrap an angle to (-PI, PI].
fn wrap_to_pi(mut a: f64) -> f64 {
    while a <= -PI {
        a += 2.0 * PI;
    }
    while a > PI {
        a -= 2.0 * PI;
    }
    a
}

/// Compute placements for `n` previews on a circle inside a `screen_w` x `screen_h`
/// area. `focus_pos` is the (possibly mid-animation, fractional) index of the
/// focused preview; the focused preview sits at the top of the circle.
/// `aspect` is the monitor aspect ratio (width / height) used for preview shape.
pub fn compute(
    n: usize,
    focus_pos: f64,
    screen_w: f64,
    screen_h: f64,
    aspect: f64,
    p: &RingParams,
) -> Vec<Placement> {
    if n == 0 {
        return Vec::new();
    }

    let cx = screen_w / 2.0;
    let cy = screen_h / 2.0;

    // Focused preview size, clamped so it can never dominate a tiny screen.
    let w_f = (p.focus_width_frac * screen_w).max(64.0);
    let h_f = w_f / aspect;

    if n == 1 {
        return vec![Placement {
            x: cx - w_f / 2.0,
            y: cy - h_f / 2.0,
            width: w_f,
            height: h_f,
            scale: p.s_max,
        }];
    }

    let radius = ((screen_w.min(screen_h)) / 2.0 - w_f.max(h_f) / 2.0 - p.margin).max(0.0);

    (0..n)
        .map(|i| {
            let theta = -PI / 2.0 + (i as f64 - focus_pos) * 2.0 * PI / n as f64;
            let delta = wrap_to_pi(theta + PI / 2.0);
            let t = ((1.0 + delta.cos()) / 2.0).powf(p.falloff);
            let scale = p.s_min + (p.s_max - p.s_min) * t;
            let w = w_f * scale;
            let h = h_f * scale;
            Placement {
                x: cx + radius * theta.cos() - w / 2.0,
                y: cy + radius * theta.sin() - h / 2.0,
                width: w,
                height: h,
                scale,
            }
        })
        .collect()
}

/// Shortest signed step to animate from `from` to index `to` on a ring of `n`.
pub fn shortest_step(from: f64, to: usize, n: usize) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let n = n as f64;
    let mut d = (to as f64 - from) % n;
    if d > n / 2.0 {
        d -= n;
    } else if d < -n / 2.0 {
        d += n;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f64 = 1920.0;
    const H: f64 = 1200.0;
    const ASPECT: f64 = 1920.0 / 1200.0;

    #[test]
    fn focused_is_largest_and_on_top() {
        let p = RingParams::default();
        let places = compute(6, 2.0, W, H, ASPECT, &p);
        let focused = &places[2];
        for (i, pl) in places.iter().enumerate() {
            if i != 2 {
                assert!(pl.scale < focused.scale, "preview {i} not smaller than focus");
            }
        }
        // Focused preview centers near the top of the circle (above screen center).
        assert!(focused.y + focused.height / 2.0 < H / 2.0);
        // ... and horizontally centered.
        let cx = focused.x + focused.width / 2.0;
        assert!((cx - W / 2.0).abs() < 1e-6);
    }

    #[test]
    fn everything_fits_on_screen() {
        let p = RingParams::default();
        for n in 1..=10 {
            for f in 0..n {
                for pl in compute(n, f as f64, W, H, ASPECT, &p) {
                    assert!(pl.x >= 0.0 && pl.y >= 0.0, "n={n} f={f} {pl:?}");
                    assert!(pl.x + pl.width <= W, "n={n} f={f} {pl:?}");
                    assert!(pl.y + pl.height <= H, "n={n} f={f} {pl:?}");
                }
            }
        }
    }

    #[test]
    fn single_workspace_is_centered() {
        let p = RingParams::default();
        let places = compute(1, 0.0, W, H, ASPECT, &p);
        assert_eq!(places.len(), 1);
        let pl = &places[0];
        assert!((pl.x + pl.width / 2.0 - W / 2.0).abs() < 1e-6);
        assert!((pl.y + pl.height / 2.0 - H / 2.0).abs() < 1e-6);
    }

    #[test]
    fn opposite_preview_has_min_scale() {
        let p = RingParams::default();
        let places = compute(4, 0.0, W, H, ASPECT, &p);
        assert!((places[0].scale - p.s_max).abs() < 1e-9);
        assert!((places[2].scale - p.s_min).abs() < 1e-9);
    }

    #[test]
    fn shortest_step_wraps() {
        assert_eq!(shortest_step(0.0, 1, 6), 1.0);
        assert_eq!(shortest_step(0.0, 5, 6), -1.0);
        assert_eq!(shortest_step(5.0, 0, 6), 1.0);
        assert_eq!(shortest_step(1.0, 3, 6), 2.0);
    }
}
