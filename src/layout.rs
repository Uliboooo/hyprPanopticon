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
    /// Non-uniform angular spacing, 0..=1. 0 spaces previews evenly around
    /// the circle; higher values give the large previews near the focus more
    /// room and pack the small ones together at the bottom.
    pub spread: f64,
    /// Pull shrunken previews toward the vertical center line, 0..=1. Only
    /// the horizontal position is affected: side previews slide inward and
    /// fill the middle of the ring, while top/bottom previews stay put.
    pub center_pull: f64,
}

impl Default for RingParams {
    fn default() -> Self {
        Self {
            s_max: 1.0,
            s_min: 0.6,
            falloff: 1.2,
            focus_width_frac: 0.32,
            margin: 24.0,
            spread: 0.25,
            center_pull: 0.0,
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

/// Compute placements for `n` previews on a circle inside a `screen_w` x `screen_h`
/// area. `focus_pos` is the (possibly mid-animation, fractional) index of the
/// focused preview; the focused preview sits at the top of the circle.
/// `aspect` is the monitor aspect ratio (width / height) used for preview shape.
/// `left_inset` reserves space from the left edge (e.g. for the special-
/// workspace column); previews on the left half of the circle stay clear of it.
pub fn compute(
    n: usize,
    focus_pos: f64,
    screen_w: f64,
    screen_h: f64,
    aspect: f64,
    left_inset: f64,
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

    // Angle and scale per preview. `u` is the ring-position offset from the
    // focus in [-0.5, 0.5). Scale falls off with |u|; the angle warps by
    // `spread` so large previews near the focus get more angular room while
    // small ones bunch up at the bottom. Both depend only on u, so the
    // (angle, scale) pairing — and therefore the radius below — is stable
    // while the ring rotates.
    let spread = p.spread.clamp(0.0, 1.0);
    let items: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let mut u = (i as f64 - focus_pos) / n as f64;
            u -= u.round(); // wrap to [-0.5, 0.5]
            let delta = 2.0 * PI * u;
            let theta = -PI / 2.0 + delta + spread * delta.sin();
            let t = ((1.0 + delta.cos()) / 2.0).powf(p.falloff);
            (theta, p.s_min + (p.s_max - p.s_min) * t)
        })
        .collect();

    // Horizontal pull factor per item: shrunken previews slide toward the
    // vertical center line so the middle of the ring doesn't stay hollow.
    // The vertical position keeps the full radius, so the bottom previews
    // stay anchored near the bottom edge.
    let pull = p.center_pull.clamp(0.0, 1.0);
    let scale_span = (p.s_max - p.s_min).max(1e-9);
    let pull_factor = |scale: f64| {
        let s_norm = ((scale - p.s_min) / scale_span).clamp(0.0, 1.0);
        1.0 - pull * (1.0 - s_norm)
    };

    // Fit the ring: `fit` uniformly shrinks every preview until, on the
    // largest ellipse where everything still stays on screen (and clear of
    // the reserved left strip), ring-order neighbors keep a visible gap.
    // The ellipse (rather than a circle) matters on wide screens: a circle
    // is capped by the screen height, which crowds the diagonal previews
    // toward the vertical centerline and pinches the middle opening into a
    // ragged slit. Stretching horizontally keeps the opening a clean oval.
    // Shrinking also frees screen space (bigger radii), so the two are
    // coupled; feasibility is monotonic in `fit`, found by bisection.
    let left_reserved = left_inset.max(p.margin);
    // Neighbors may overlap by this fraction of their combined extent;
    // trading a bit of overlap keeps the previews larger and readable.
    let overlap_allowance = 0.25;

    let radii_for = |fit: f64| -> (f64, f64) {
        let mut rx = f64::INFINITY;
        let mut ry = f64::INFINITY;
        for &(theta, scale) in &items {
            let (w, h) = (w_f * fit * scale, h_f * fit * scale);
            let (cos, sin) = (theta.cos(), theta.sin());
            let k = pull_factor(scale).max(1e-6);
            let avail_x = if cos < 0.0 {
                cx - left_reserved - w / 2.0
            } else {
                cx - p.margin - w / 2.0
            };
            let avail_y = screen_h / 2.0 - p.margin - h / 2.0;
            if cos.abs() > 1e-6 {
                rx = rx.min(avail_x.max(0.0) / (k * cos.abs()));
            }
            if sin.abs() > 1e-6 {
                ry = ry.min(avail_y.max(0.0) / sin.abs());
            }
        }
        // Unconstrained axis (e.g. n = 2 stacked vertically): mirror the
        // other one. Cap the aspect so the ring still reads as a ring.
        if !rx.is_finite() {
            rx = ry;
        }
        if !ry.is_finite() {
            ry = rx;
        }
        if !rx.is_finite() {
            return (0.0, 0.0);
        }
        (rx.min(1.8 * ry), ry.min(1.8 * rx))
    };

    // Neighbor spacing on the ellipse, measured along the axis through both
    // centers: the center distance versus the sum of both rects' half-extent
    // projections onto it (their combined footprint), discounted by the
    // allowed overlap. `theta` is monotonic in ring position, so index
    // neighbors are the geometric neighbors.
    let gaps_ok = |fit: f64, (rx, ry): (f64, f64)| -> bool {
        let center = |theta: f64, scale: f64| {
            (
                rx * pull_factor(scale) * theta.cos(),
                ry * theta.sin(),
            )
        };
        (0..n).all(|i| {
            let (t0, s0) = items[i];
            let (t1, s1) = items[(i + 1) % n];
            let (x0, y0) = center(t0, s0);
            let (x1, y1) = center(t1, s1);
            let dist = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
            if dist < 1e-6 {
                return false;
            }
            let (dx, dy) = ((x1 - x0).abs() / dist, (y1 - y0).abs() / dist);
            let half = |s: f64| (w_f * fit * s / 2.0) * dx + (h_f * fit * s / 2.0) * dy;
            dist >= (half(s0) + half(s1)) * (1.0 - overlap_allowance)
        })
    };

    let mut fit = 1.0;
    if !gaps_ok(1.0, radii_for(1.0)) {
        // Below the lower bound the previews would be unusably small; accept
        // a cramped ring instead.
        let (mut lo, mut hi) = (0.25, 1.0);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if gaps_ok(mid, radii_for(mid)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        fit = lo;
    }
    let (rx, ry) = radii_for(fit);

    items
        .into_iter()
        .map(|(theta, scale)| {
            let w = w_f * fit * scale;
            let h = h_f * fit * scale;
            Placement {
                x: cx + rx * pull_factor(scale) * theta.cos() - w / 2.0,
                y: cy + ry * theta.sin() - h / 2.0,
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
        let places = compute(6, 2.0, W, H, ASPECT, 0.0, &p);
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
                for pl in compute(n, f as f64, W, H, ASPECT, 0.0, &p) {
                    assert!(pl.x >= 0.0 && pl.y >= 0.0, "n={n} f={f} {pl:?}");
                    assert!(pl.x + pl.width <= W, "n={n} f={f} {pl:?}");
                    assert!(pl.y + pl.height <= H, "n={n} f={f} {pl:?}");
                }
            }
        }
    }

    #[test]
    fn respects_left_inset() {
        let p = RingParams::default();
        let inset = 280.0;
        for n in 2..=10 {
            for f in 0..n {
                for pl in compute(n, f as f64, W, H, ASPECT, inset, &p) {
                    // Only previews on the left half are constrained by the
                    // inset; the focused one sits at the top center.
                    assert!(pl.x >= 0.0, "n={n} f={f} {pl:?}");
                    let center_x = pl.x + pl.width / 2.0;
                    if center_x < W / 2.0 - 1.0 {
                        assert!(pl.x >= inset - 1e-6, "n={n} f={f} {pl:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn radius_grows_versus_worst_case() {
        // The per-angle fitting must beat the old worst-case radius so the
        // ring actually spreads out (measured without the center pull).
        let p = RingParams { center_pull: 0.0, ..RingParams::default() };
        let places = compute(8, 0.0, W, H, ASPECT, 0.0, &p);
        let w_f = p.focus_width_frac * W;
        let old_radius = H.min(W) / 2.0 - w_f.max(w_f / ASPECT) / 2.0 - p.margin;
        // Bottom-most preview center distance from screen center.
        let bottom = &places[4];
        let dist = (bottom.y + bottom.height / 2.0 - H / 2.0).abs();
        assert!(dist > old_radius + 50.0, "radius did not grow: {dist} vs {old_radius}");
    }

    #[test]
    fn center_pull_fills_the_middle() {
        let flat = RingParams { center_pull: 0.0, ..RingParams::default() };
        let pulled = RingParams { center_pull: 0.5, ..RingParams::default() };
        // A small side preview slides toward the vertical center line…
        let side_dist = |p: &RingParams| {
            let places = compute(8, 0.0, W, H, ASPECT, 0.0, p);
            let s = &places[2];
            (s.x + s.width / 2.0 - W / 2.0).abs()
        };
        let (d_flat, d_pulled) = (side_dist(&flat), side_dist(&pulled));
        assert!(
            d_pulled < d_flat,
            "pull did not move side previews inward: {d_pulled} vs {d_flat}"
        );
        // …while the bottom preview stays anchored in the lower band (pulling
        // only affects horizontal positions; the fitted radius may differ
        // slightly between the two configs).
        let places = compute(8, 0.0, W, H, ASPECT, 0.0, &pulled);
        let bottom_y = places[4].y + places[4].height / 2.0;
        assert!(bottom_y > H * 0.7, "bottom preview drifted up: {bottom_y}");
    }

    #[test]
    fn single_workspace_is_centered() {
        let p = RingParams::default();
        let places = compute(1, 0.0, W, H, ASPECT, 0.0, &p);
        assert_eq!(places.len(), 1);
        let pl = &places[0];
        assert!((pl.x + pl.width / 2.0 - W / 2.0).abs() < 1e-6);
        assert!((pl.y + pl.height / 2.0 - H / 2.0).abs() < 1e-6);
    }

    #[test]
    fn opposite_preview_has_min_scale() {
        let p = RingParams::default();
        let places = compute(4, 0.0, W, H, ASPECT, 0.0, &p);
        assert!((places[0].scale - p.s_max).abs() < 1e-9);
        assert!((places[2].scale - p.s_min).abs() < 1e-9);
    }

    #[test]
    fn default_layout_is_an_open_ring() {
        // The default look is a circle with breathing room: previews sit on
        // the ring, never pile up in the middle, and keep gaps between them.
        let p = RingParams::default();
        for n in 2..=12 {
            let places = compute(n, 0.0, W, H, ASPECT, 0.0, &p);
            // The middle of the ring stays open: no preview center near the
            // screen center.
            for pl in &places {
                let (cx, cy) = (pl.x + pl.width / 2.0, pl.y + pl.height / 2.0);
                let dist = ((cx - W / 2.0).powi(2) + (cy - H / 2.0).powi(2)).sqrt();
                assert!(dist > H * 0.2, "n={n}: preview sits in the middle: {pl:?}");
            }
            // Moderate overlap is allowed, but no preview may be buried:
            // any pairwise intersection stays under 40% of the smaller one.
            for i in 0..places.len() {
                for j in i + 1..places.len() {
                    let (a, b) = (&places[i], &places[j]);
                    let ox = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
                    let oy = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
                    let inter = ox.max(0.0) * oy.max(0.0);
                    let smaller = (a.width * a.height).min(b.width * b.height);
                    assert!(
                        inter < 0.4 * smaller,
                        "n={n}: previews {i} and {j} overlap too much: {a:?} {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn shortest_step_wraps() {
        assert_eq!(shortest_step(0.0, 1, 6), 1.0);
        assert_eq!(shortest_step(0.0, 5, 6), -1.0);
        assert_eq!(shortest_step(5.0, 0, 6), 1.0);
        assert_eq!(shortest_step(1.0, 3, 6), 2.0);
    }
}
