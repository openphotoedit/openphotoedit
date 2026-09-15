//! Distort: Twirl, Pinch, Spherize, Ripple, Polar Coordinates.
//!
//! Each is an inverse map: for every output pixel, where in the source to
//! sample (bilinear, premultiplied). Geometry is relative to the filtered
//! area — the selection's bounds or the layer.

use crate::util::{put_premultiplied, Frame, Sampler};
use crate::RippleSize;

fn warp(buf: &mut [u8], frame: &Frame, wrap_x: bool, map: impl Fn(f32, f32) -> Option<(f32, f32)>) {
    let (w, h) = (frame.w, frame.h);
    let src = buf.to_vec();
    let s = Sampler { src: &src, w, h };
    let inner = frame.inner;
    for y in inner.y as usize..inner.bottom() as usize {
        for x in inner.x as usize..inner.right() as usize {
            let Some((sx, sy)) = map(x as f32 + 0.5, y as f32 + 0.5) else { continue };
            let p = if wrap_x { s.sample_wrap_x(sx, sy) } else { s.sample(sx, sy) };
            let i = (y * w + x) * 4;
            put_premultiplied(&mut buf[i..i + 4], p);
        }
    }
}

fn centre(frame: &Frame) -> (f32, f32, f32) {
    let r = frame.inner;
    (r.x as f32 + r.w as f32 / 2.0, r.y as f32 + r.h as f32 / 2.0, r.w.min(r.h) as f32 / 2.0)
}

/// Twirl: rotation that is strongest at the centre and fades to nothing at
/// the edge of the inscribed circle.
pub fn twirl(buf: &mut [u8], frame: &Frame, angle: f32) {
    if angle == 0.0 {
        return;
    }
    let (cx, cy, radius) = centre(frame);
    if radius < 1.0 {
        return;
    }
    let a = angle.to_radians();
    warp(buf, frame, false, |x, y| {
        let (dx, dy) = (x - cx, y - cy);
        let d = (dx * dx + dy * dy).sqrt() / radius;
        if d >= 1.0 {
            return None;
        }
        let t = (1.0 - d) * (1.0 - d) * a;
        let (s, c) = t.sin_cos();
        Some((cx + dx * c - dy * s, cy + dx * s + dy * c))
    });
}

/// Pinch: positive squeezes the centre inward, negative bulges it.
pub fn pinch(buf: &mut [u8], frame: &Frame, amount: f32) {
    if amount == 0.0 {
        return;
    }
    let (cx, cy, radius) = centre(frame);
    if radius < 1.0 {
        return;
    }
    let p = amount / 100.0;
    warp(buf, frame, false, |x, y| {
        let (dx, dy) = (x - cx, y - cy);
        let d = (dx * dx + dy * dy).sqrt() / radius;
        if d >= 1.0 || d <= 0.0 {
            return None;
        }
        // Sampling further out shrinks the picture towards the centre. The
        // exponent stays below 1 so the centre never collapses to a ring.
        let f = (std::f32::consts::FRAC_PI_2 * d).sin().powf(-0.6 * p);
        Some((cx + dx * f, cy + dy * f))
    });
}

/// Spherize: wraps the inscribed circle around a sphere (positive) or
/// pushes it into a bowl (negative).
pub fn spherize(buf: &mut [u8], frame: &Frame, amount: f32) {
    if amount == 0.0 {
        return;
    }
    let (cx, cy, radius) = centre(frame);
    if radius < 1.0 {
        return;
    }
    let a = amount / 100.0;
    warp(buf, frame, false, |x, y| {
        let (dx, dy) = (x - cx, y - cy);
        let d = (dx * dx + dy * dy).sqrt() / radius;
        if d >= 1.0 || d <= 0.0 {
            return None;
        }
        let target = if a > 0.0 { d + (d.asin() * std::f32::consts::FRAC_2_PI - d) * a } else { d + ((d * std::f32::consts::FRAC_PI_2).sin() - d) * -a };
        let f = target / d;
        Some((cx + dx * f, cy + dy * f))
    });
}

/// Wavelength and displacement for Ripple's `amount` (Photoshop's -999..999)
/// and size.
pub fn ripple_params(amount: f32, size: RippleSize) -> (f32, f32) {
    let wave = match size {
        RippleSize::Small => 10.0,
        RippleSize::Medium => 24.0,
        RippleSize::Large => 48.0,
    };
    (wave, amount / 100.0 * wave * 0.08)
}

/// Ripple: sinusoidal displacement, horizontal driven by the row and
/// vertical by the column, in document coordinates so tiles line up.
pub fn ripple(buf: &mut [u8], frame: &Frame, amount: f32, size: RippleSize) {
    if amount == 0.0 {
        return;
    }
    let (wave, amp) = ripple_params(amount, size);
    let k = std::f32::consts::TAU / wave;
    let (ox, oy) = (frame.outer.x as f32, frame.outer.y as f32);
    warp(buf, frame, false, |x, y| {
        let (docx, docy) = (x + ox, y + oy);
        Some((x + amp * (docy * k).sin(), y + amp * (docx * k).sin()))
    });
}

/// Polar Coordinates. Rectangular → polar bends the top edge into the centre
/// and the sides into a seam at twelve o'clock; polar → rectangular unrolls
/// it again.
pub fn polar(buf: &mut [u8], frame: &Frame, to_polar: bool) {
    let r = frame.inner;
    let (x0, y0, w, h) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
    if w < 2.0 || h < 2.0 {
        return;
    }
    let (cx, cy) = (x0 + w / 2.0, y0 + h / 2.0);
    let tau = std::f32::consts::TAU;
    if to_polar {
        warp(buf, frame, false, |x, y| {
            let nx = (x - cx) / (w / 2.0);
            let ny = (y - cy) / (h / 2.0);
            let rad = (nx * nx + ny * ny).sqrt();
            let theta = nx.atan2(-ny).rem_euclid(tau);
            let sx = x0 + theta / tau * w;
            let sy = y0 + rad * h;
            // Wrap horizontally within the area so the seam is continuous.
            let sx = x0 + (sx - x0).rem_euclid(w);
            Some((sx, sy.min(y0 + h - 0.5)))
        });
    } else {
        warp(buf, frame, false, |x, y| {
            let theta = (x - x0) / w * tau;
            let rad = (y - y0) / h;
            let sx = cx + theta.sin() * rad * w / 2.0;
            let sy = cy - theta.cos() * rad * h / 2.0;
            Some((sx, sy))
        });
    }
}
