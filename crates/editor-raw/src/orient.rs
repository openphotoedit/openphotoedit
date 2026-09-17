//! EXIF orientation applied to an interleaved buffer.

/// Size after applying orientation `o` (1..8) to a `w`×`h` image.
pub fn oriented_size(w: usize, h: usize, o: u8) -> (usize, usize) {
    if (5..=8).contains(&o) {
        (h, w)
    } else {
        (w, h)
    }
}

/// Reorient `src` (`w`×`h`, `cpp` samples per pixel) per EXIF orientation.
///
/// 1 normal, 2 mirror horizontal, 3 rotate 180, 4 mirror vertical,
/// 5 transpose, 6 rotate 90 CW, 7 transverse, 8 rotate 90 CCW.
pub fn apply<T: Copy + Default + Send + Sync>(src: &[T], w: usize, h: usize, cpp: usize, o: u8) -> (Vec<T>, usize, usize) {
    use rayon::prelude::*;
    if !(2..=8).contains(&o) {
        return (src.to_vec(), w, h);
    }
    let (ow, oh) = oriented_size(w, h, o);
    let mut out = vec![T::default(); ow * oh * cpp];
    out.par_chunks_mut(ow * cpp).enumerate().for_each(|(oy, row)| {
        for ox in 0..ow {
            // Source pixel for output (ox, oy).
            let (sx, sy) = match o {
                2 => (w - 1 - ox, oy),
                3 => (w - 1 - ox, h - 1 - oy),
                4 => (ox, h - 1 - oy),
                5 => (oy, ox),
                6 => (oy, h - 1 - ox),
                7 => (w - 1 - oy, h - 1 - ox),
                8 => (w - 1 - oy, ox),
                _ => (ox, oy),
            };
            let s = (sy * w + sx) * cpp;
            row[ox * cpp..(ox + 1) * cpp].copy_from_slice(&src[s..s + cpp]);
        }
    });
    (out, ow, oh)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 3×2 image:
    // a b c
    // d e f
    const IMG: [char; 6] = ['a', 'b', 'c', 'd', 'e', 'f'];

    fn run(o: u8) -> (String, usize, usize) {
        let (v, w, h) = apply(&IMG, 3, 2, 1, o);
        (v.into_iter().collect(), w, h)
    }

    #[test]
    fn all_orientations() {
        assert_eq!(run(1), ("abcdef".into(), 3, 2));
        assert_eq!(run(2), ("cbafed".into(), 3, 2));
        assert_eq!(run(3), ("fedcba".into(), 3, 2));
        assert_eq!(run(4), ("defabc".into(), 3, 2));
        assert_eq!(run(5), ("adbecf".into(), 2, 3));
        // Rotate 90° clockwise: left column becomes the top row, reversed.
        assert_eq!(run(6), ("daebfc".into(), 2, 3));
        assert_eq!(run(7), ("fcebda".into(), 2, 3));
        // Rotate 90° counter-clockwise.
        assert_eq!(run(8), ("cfbead".into(), 2, 3));
    }
}
