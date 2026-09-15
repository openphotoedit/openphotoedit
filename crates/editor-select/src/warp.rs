//! Transform Selection: an affine warp of the selection mask with bilinear
//! resampling. Pixels mapped from outside the canvas are unselected.

use editor_core::geom::{Affine, Point};
use editor_core::plane::Plane;

use crate::morph::selection_area;

pub fn transform(sel: &Plane, m: &Affine) -> Option<Plane> {
    let inv = m.invert()?;
    let canvas = sel.bounds();
    let (cw, ch) = (canvas.w, canvas.h);
    let mut out = Plane::mask(cw as u32, ch as u32, 0);
    let src_area = selection_area(sel);
    if src_area.is_empty() {
        return Some(out);
    }
    let dst = m.bounds(&src_area).inflate(1).intersect(&canvas);
    if dst.is_empty() {
        return Some(out);
    }
    let read = src_area.inflate(1).intersect(&canvas);
    let src = sel.read_vec(read);
    let at = |x: i32, y: i32| -> f32 {
        if x < read.x || y < read.y || x >= read.right() || y >= read.bottom() {
            0.0
        } else {
            src[((y - read.y) * read.w + (x - read.x)) as usize] as f32
        }
    };
    let mut buf = vec![0u8; dst.area() as usize];
    for y in 0..dst.h {
        for x in 0..dst.w {
            let p = inv.apply(Point::new((dst.x + x) as f64 + 0.5, (dst.y + y) as f64 + 0.5));
            let (sx, sy) = (p.x - 0.5, p.y - 0.5);
            let (fx, fy) = (sx.floor(), sy.floor());
            let (ix, iy) = (fx as i32, fy as i32);
            let (tx, ty) = ((sx - fx) as f32, (sy - fy) as f32);
            let v = at(ix, iy) * (1.0 - tx) * (1.0 - ty) + at(ix + 1, iy) * tx * (1.0 - ty) + at(ix, iy + 1) * (1.0 - tx) * ty + at(ix + 1, iy + 1) * tx * ty;
            buf[(y * dst.w + x) as usize] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    out.write(dst, &buf);
    out.compact();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::geom::Rect;

    #[test]
    fn translate_moves_pixels_exactly() {
        let mut sel = Plane::mask(50, 50, 0);
        sel.write(Rect::new(10, 10, 5, 5), &[255; 25]);
        let t = transform(&sel, &Affine::translate(7.0, -3.0)).unwrap();
        assert_eq!(t.content_bounds(), Rect::new(17, 7, 5, 5));
        assert_eq!(t.get(19, 9)[0], 255);
    }

    #[test]
    fn scale_about_origin_doubles_extent() {
        let mut sel = Plane::mask(50, 50, 0);
        sel.write(Rect::new(4, 4, 8, 8), &[255; 64]);
        let t = transform(&sel, &Affine::scale(2.0, 2.0)).unwrap();
        assert_eq!(t.get(16, 16)[0], 255);
        assert_eq!(t.get(6, 6)[0], 0);
        let b = t.content_bounds();
        assert!(b.x >= 7 && b.x <= 8 && b.right() >= 24 && b.right() <= 25, "{b:?}");
    }

    #[test]
    fn select_all_rotated_leaves_corners_unselected() {
        let sel = Plane::mask(40, 40, 255);
        let m = Affine::translate(-20.0, -20.0).then(&Affine::rotate(0.5)).then(&Affine::translate(20.0, 20.0));
        let t = transform(&sel, &m).unwrap();
        assert_eq!(t.get(20, 20)[0], 255);
        assert_eq!(t.get(0, 0)[0], 0);
    }

    #[test]
    fn singular_matrix_is_rejected() {
        let sel = Plane::mask(4, 4, 255);
        assert!(transform(&sel, &Affine::scale(0.0, 1.0)).is_none());
    }
}
