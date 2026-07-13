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
            s_min: 0.45,
            falloff: 2.0,
            focus_width_frac: 0.34,
            margin: 24.0,
            spread: 0.7,
            center_pull: 0.4,
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

    // Largest radius at which every preview still fits on screen (and clear
    // of the reserved left strip), given its own size, angle, and pull.
    let left_reserved = left_inset.max(p.margin);
    let mut radius = f64::INFINITY;
    for &(theta, scale) in &items {
        let (w, h) = (w_f * scale, h_f * scale);
        let (cos, sin) = (theta.cos(), theta.sin());
        let k = pull_factor(scale).max(1e-6);
        let avail_x = if cos < 0.0 {
            cx - left_reserved - w / 2.0
        } else {
            cx - p.margin - w / 2.0
        };
        let avail_y = screen_h / 2.0 - p.margin - h / 2.0;
        if cos.abs() > 1e-6 {
            radius = radius.min(avail_x.max(0.0) / (k * cos.abs()));
        }
        if sin.abs() > 1e-6 {
            radius = radius.min(avail_y.max(0.0) / sin.abs());
        }
    }
    if !radius.is_finite() {
        radius = 0.0;
    }

    items
        .into_iter()
        .map(|(theta, scale)| {
            let w = w_f * scale;
            let h = h_f * scale;
            Placement {
                x: cx + radius * pull_factor(scale) * theta.cos() - w / 2.0,
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
        // …while the bottom preview keeps its vertical position.
        let bottom_y = |p: &RingParams| {
            let places = compute(8, 0.0, W, H, ASPECT, 0.0, p);
            places[4].y + places[4].height / 2.0
        };
        assert!((bottom_y(&flat) - bottom_y(&pulled)).abs() < 1e-6);
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
    fn default_layout_fills_center_and_bottom() {
        // Regression for the two reported holes: a hollow ring middle, and
        // (after the first pull attempt) an empty bottom band.
        let p = RingParams::default();
        let places = compute(10, 0.0, W, H, ASPECT, 0.0, &p);
        // Bottom band is used: some preview reaches the lower part of the screen.
        let lowest = places
            .iter()
            .map(|pl| pl.y + pl.height)
            .fold(0.0f64, f64::max);
        assert!(lowest > H * 0.85, "bottom band empty, lowest edge {lowest}");
        // Center band is used: some preview center lands near the middle.
        let center_used = places.iter().any(|pl| {
            let (cx, cy) = (pl.x + pl.width / 2.0, pl.y + pl.height / 2.0);
            (cx - W / 2.0).abs() < W * 0.17 && (cy - H / 2.0).abs() < H * 0.22
        });
        assert!(center_used, "middle of the ring is hollow: {places:#?}");
    }

    #[test]
    fn shortest_step_wraps() {
        assert_eq!(shortest_step(0.0, 1, 6), 1.0);
        assert_eq!(shortest_step(0.0, 5, 6), -1.0);
        assert_eq!(shortest_step(5.0, 0, 6), 1.0);
        assert_eq!(shortest_step(1.0, 3, 6), 2.0);
    }
}
