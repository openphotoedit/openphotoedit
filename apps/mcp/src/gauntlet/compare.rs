//! The comparison itself: line our composite up with Photoshop's, measure the
//! error, classify it, and make three small pictures of it.
//!
//! Every number here is defined so that it matches
//! `crates/editor-psd/tests/composite_diff.rs`, which is the reference for
//! how this repository measures composite fidelity. In particular
//! [`over_white`] and [`Metrics::mae`] are that test's `over_white` and `mae`
//! verbatim, so a tally produced here and a tally produced there describe the
//! same thing. If they ever stop agreeing, one of them has a bug.
//!
//! Nothing in this module rounds a failure away. A file with no reference
//! composite is [`Verdict::StructureOnly`], not a pass; a file that panics the
//! importer is [`Verdict::Failed`], not a skip.

use std::borrow::Cow;

use base64::Engine as _;

/// Longest side of the JPEGs that go down the wire, in pixels.
pub const THUMB_MAX_SIDE: u32 = 256;
/// JPEG quality for those thumbnails.
pub const THUMB_QUALITY: u8 = 60;
/// The difference map is multiplied by this before it is shown. An error of
/// one level out of 255 is invisible otherwise, and the whole point of the
/// panel is to make the disagreement visible; ×4 is what `composite_diff.rs`
/// uses, so the two read the same.
pub const DIFF_AMPLIFY: f32 = 4.0;

// --- the aligner ---------------------------------------------------------

/// A picture to compare: straight (non-premultiplied) RGBA, `width * height *
/// 4` bytes.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

impl<'a> Frame<'a> {
    pub fn new(width: u32, height: u32, rgba: &'a [u8]) -> Frame<'a> {
        Frame {
            width,
            height,
            rgba,
        }
    }

    fn is_well_formed(&self) -> bool {
        let want = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(4));
        self.width > 0 && self.height > 0 && want == Some(self.rgba.len())
    }
}

/// Two frames cut to a common rectangle, ready to be measured.
#[derive(Debug)]
pub struct Aligned<'a> {
    pub width: u32,
    pub height: u32,
    pub ours: Cow<'a, [u8]>,
    pub theirs: Cow<'a, [u8]>,
    /// True when the two frames were not the same size and both were cropped
    /// to the overlap. Worth saying out loud in the UI: a cropped comparison
    /// measures less than the whole picture.
    pub cropped: bool,
}

/// Why two frames could not be lined up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignError {
    /// There is no reference composite at all — the file did not carry one,
    /// or carried a placeholder. Not a comparison, and not a pass.
    NoComposite,
    /// A frame has zero area, or its buffer is not `width * height * 4`.
    Malformed,
}

impl std::fmt::Display for AlignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlignError::NoComposite => f.write_str("the file carries no reference composite"),
            AlignError::Malformed => f.write_str("a frame is empty or the wrong length"),
        }
    }
}

/// Line two frames up at the top-left corner.
///
/// Same size is the overwhelmingly common case and costs nothing — the
/// buffers are borrowed. Different sizes are cropped to the overlap, which is
/// what a human comparing two renders by eye would do, and the result says so
/// through [`Aligned::cropped`]. An empty overlap is [`AlignError::Malformed`]
/// rather than a comparison of nothing.
pub fn align<'a>(ours: Frame<'a>, theirs: Option<Frame<'a>>) -> Result<Aligned<'a>, AlignError> {
    let theirs = theirs.ok_or(AlignError::NoComposite)?;
    if !ours.is_well_formed() || !theirs.is_well_formed() {
        return Err(AlignError::Malformed);
    }
    if ours.width == theirs.width && ours.height == theirs.height {
        return Ok(Aligned {
            width: ours.width,
            height: ours.height,
            ours: Cow::Borrowed(ours.rgba),
            theirs: Cow::Borrowed(theirs.rgba),
            cropped: false,
        });
    }
    let w = ours.width.min(theirs.width);
    let h = ours.height.min(theirs.height);
    if w == 0 || h == 0 {
        return Err(AlignError::Malformed);
    }
    Ok(Aligned {
        width: w,
        height: h,
        ours: Cow::Owned(crop(ours, w, h)),
        theirs: Cow::Owned(crop(theirs, w, h)),
        cropped: true,
    })
}

fn crop(frame: Frame<'_>, w: u32, h: u32) -> Vec<u8> {
    let (sw, w, h) = (frame.width as usize, w as usize, h as usize);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = (y * sw) * 4;
        out.extend_from_slice(&frame.rgba[row..row + w * 4]);
    }
    out
}

// --- the measurement -----------------------------------------------------

/// Composite one straight-RGBA pixel over white, as `composite_diff.rs` does.
///
/// Over white rather than over black because that is what Photoshop's own
/// merged image is: an opaque picture. Comparing premultiplied pixels would
/// score two completely different colours as equal wherever alpha is zero.
pub fn over_white(p: &[u8]) -> [f32; 3] {
    let a = p[3] as f32 / 255.0;
    [0, 1, 2].map(|c| p[c] as f32 * a + 255.0 * (1.0 - a))
}

/// How many bins the 95th percentile is read out of. Sorting every pixel of a
/// 24 megapixel file 244 times over is the slow way to get a percentile; a
/// fixed histogram is one pass and no allocation per pixel.
const P95_BINS: usize = 4096;

/// The error between two aligned frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Mean absolute error over RGB, on white, in levels out of 255.
    pub mae: f64,
    /// The 95th percentile of the same per-pixel quantity, reported as the
    /// lower edge of its bin — so it under-reports by at most 255/4096
    /// (0.062 levels) and never over-reports.
    pub p95: f64,
}

/// Measure two equal-length RGBA buffers.
pub fn metrics(ours: &[u8], theirs: &[u8]) -> Metrics {
    let n = ours.len().min(theirs.len()) / 4;
    if n == 0 {
        return Metrics { mae: 0.0, p95: 0.0 };
    }
    let mut sum = 0f64;
    let mut bins = vec![0u32; P95_BINS + 1];
    for (pa, pb) in ours.chunks_exact(4).zip(theirs.chunks_exact(4)) {
        let (ca, cb) = (over_white(pa), over_white(pb));
        let d = ca
            .iter()
            .zip(cb.iter())
            .map(|(x, y)| (x - y).abs() as f64)
            .sum::<f64>()
            / 3.0;
        sum += d;
        let bin = ((d / 255.0) * P95_BINS as f64) as usize;
        bins[bin.min(P95_BINS)] += 1;
    }
    // The first bin whose cumulative count reaches 95% of the pixels.
    let target = (n as f64 * 0.95).ceil() as u64;
    let mut cumulative = 0u64;
    let mut p95_bin = P95_BINS;
    for (i, count) in bins.iter().enumerate() {
        cumulative += *count as u64;
        if cumulative >= target {
            p95_bin = i;
            break;
        }
    }
    Metrics {
        mae: sum / n as f64,
        p95: p95_bin as f64 * 255.0 / P95_BINS as f64,
    }
}

// --- the classifier ------------------------------------------------------

/// What a file's comparison came to.
///
/// The five pixel verdicts are disjoint bands, not the cumulative counts
/// `composite_diff.rs` prints: its "≤ 3: 163" is this module's
/// `exact + within1 + within3`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Not one level out, anywhere.
    Exact,
    /// Mean absolute error over 0 and at most 1 level.
    Within1,
    /// Over 1 and at most 3.
    Within3,
    /// Over 3 and at most 10.
    Within10,
    /// Over 10 levels — a visible disagreement.
    Over,
    /// The structure parsed and we rendered something, but the file carries
    /// no composite to compare against.
    StructureOnly,
    /// The import or the render did not complete.
    Failed,
}

impl Verdict {
    /// The key this verdict counts under in the streamed state.
    pub const fn key(self) -> &'static str {
        match self {
            Verdict::Exact => "exact",
            Verdict::Within1 => "within1",
            Verdict::Within3 => "within3",
            Verdict::Within10 => "within10",
            Verdict::Over => "over",
            Verdict::StructureOnly => "structureOnly",
            Verdict::Failed => "failed",
        }
    }

    /// Whether this verdict came from an actual pixel comparison.
    pub const fn is_comparison(self) -> bool {
        !matches!(self, Verdict::StructureOnly | Verdict::Failed)
    }
}

/// Band a mean absolute error.
///
/// A NaN — which cannot arise from 8-bit input, but would be the one value
/// that quietly passed every `<=` test below — lands in [`Verdict::Over`]. A
/// number we cannot read is not a number we may call a pass.
pub fn classify(mae: f64) -> Verdict {
    if mae.is_nan() {
        return Verdict::Over;
    }
    if mae <= 0.0 {
        Verdict::Exact
    } else if mae <= 1.0 {
        Verdict::Within1
    } else if mae <= 3.0 {
        Verdict::Within3
    } else if mae <= 10.0 {
        Verdict::Within10
    } else {
        Verdict::Over
    }
}

// --- the histogram -------------------------------------------------------

/// Buckets in the streamed MAE histogram.
pub const HISTOGRAM_BUCKETS: usize = 32;
/// Levels of mean absolute error per bucket. 32 × 0.5 puts the axis at
/// 0 … 15.5, with everything worse piled into the last bucket — which is the
/// right shape for this corpus, where the interesting spread is under three
/// levels and a handful of files are tens of levels out.
pub const HISTOGRAM_BUCKET_WIDTH: f64 = 0.5;

/// Which histogram bucket an error falls in.
///
/// Linear, and the top bucket is open-ended: a file 200 levels out is not
/// eight times more interesting than one 25 levels out, and losing it off the
/// end of the chart would be worse than either.
pub fn bucket_of(mae: f64) -> usize {
    // `matches!` rather than `mae <= 0.0`, because NaN compares false against
    // everything and would fall through to the division below.
    if !matches!(mae.partial_cmp(&0.0), Some(std::cmp::Ordering::Greater)) {
        return 0;
    }
    ((mae / HISTOGRAM_BUCKET_WIDTH) as usize).min(HISTOGRAM_BUCKETS - 1)
}

/// The 33 bucket boundaries, so the front end can label the axis. The last
/// one is nominal: bucket 31 holds everything from 15.5 upwards.
pub fn bucket_edges() -> Vec<f64> {
    (0..=HISTOGRAM_BUCKETS)
        .map(|i| i as f64 * HISTOGRAM_BUCKET_WIDTH)
        .collect()
}

// --- the pictures --------------------------------------------------------

/// Flatten straight RGBA onto white as packed RGB, ready to be a JPEG.
pub fn rgb_over_white(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() / 4 * 3);
    for p in rgba.chunks_exact(4) {
        for c in over_white(p) {
            out.push(c.clamp(0.0, 255.0) as u8);
        }
    }
    out
}

/// The amplified absolute difference, as packed RGB. Black is agreement.
pub fn rgb_diff(ours: &[u8], theirs: &[u8]) -> Vec<u8> {
    let n = ours.len().min(theirs.len());
    let mut out = Vec::with_capacity(n / 4 * 3);
    for (pa, pb) in ours[..n].chunks_exact(4).zip(theirs[..n].chunks_exact(4)) {
        let (ca, cb) = (over_white(pa), over_white(pb));
        for c in 0..3 {
            out.push(((ca[c] - cb[c]).abs() * DIFF_AMPLIFY).min(255.0) as u8);
        }
    }
    out
}

/// Scale a packed-RGB image to `max_side` on its longest edge and encode it
/// as a JPEG `data:` URI.
///
/// Downscaling uses a triangle filter; upscaling uses nearest neighbour at an
/// integer factor, because half this corpus is 4×4 and 8×8 test sheets whose
/// whole content is which pixel is which colour. Smoothing those would erase
/// the evidence.
pub fn jpeg_data_uri(width: u32, height: u32, rgb: &[u8], max_side: u32, quality: u8) -> Option<String> {
    let img: image::RgbImage = image::ImageBuffer::from_raw(width, height, rgb.to_vec())?;
    let long = width.max(height).max(1);
    let scaled = if long >= max_side {
        let s = max_side as f64 / long as f64;
        let nw = ((width as f64 * s).round() as u32).max(1);
        let nh = ((height as f64 * s).round() as u32).max(1);
        image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        let k = (max_side / long).clamp(1, 64);
        image::imageops::resize(&img, width * k, height * k, image::imageops::FilterType::Nearest)
    };
    let mut jpeg = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality);
    image::ImageEncoder::write_image(
        encoder,
        scaled.as_raw(),
        scaled.width(),
        scaled.height(),
        image::ExtendedColorType::Rgb8,
    )
    .ok()?;
    Some(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&jpeg)
    ))
}

/// The three pictures of one comparison, as data URIs.
pub fn thumbnails(a: &Aligned<'_>) -> (Option<String>, Option<String>, Option<String>) {
    let (w, h) = (a.width, a.height);
    let ours = jpeg_data_uri(w, h, &rgb_over_white(&a.ours), THUMB_MAX_SIDE, THUMB_QUALITY);
    let adobe = jpeg_data_uri(w, h, &rgb_over_white(&a.theirs), THUMB_MAX_SIDE, THUMB_QUALITY);
    let diff = jpeg_data_uri(w, h, &rgb_diff(&a.ours, &a.theirs), THUMB_MAX_SIDE, THUMB_QUALITY);
    (ours, adobe, diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        px.iter().copied().cycle().take((w * h * 4) as usize).collect()
    }

    // --- the classifier --------------------------------------------------

    #[test]
    fn the_bands_are_disjoint_and_closed_on_the_right() {
        assert_eq!(classify(0.0), Verdict::Exact);
        assert_eq!(classify(0.000_1), Verdict::Within1);
        assert_eq!(classify(1.0), Verdict::Within1, "1.0 is within 1");
        assert_eq!(classify(1.000_1), Verdict::Within3);
        assert_eq!(classify(3.0), Verdict::Within3);
        assert_eq!(classify(3.000_1), Verdict::Within10);
        assert_eq!(classify(10.0), Verdict::Within10);
        assert_eq!(classify(10.000_1), Verdict::Over);
        assert_eq!(classify(255.0), Verdict::Over);
    }

    #[test]
    fn an_unreadable_error_is_a_failure_not_a_pass() {
        // Every `<=` below would be false for NaN, so without the explicit
        // check this would fall through to Over anyway — but by accident.
        // The test is here so a refactor to `if mae < 1.0 {...} else if ...`
        // cannot quietly turn NaN into a pass.
        assert_eq!(classify(f64::NAN), Verdict::Over);
    }

    #[test]
    fn the_bands_sum_to_the_cumulative_counts_composite_diff_prints() {
        // composite_diff.rs reports `<=1`, `<=3`, `<=10` cumulatively. Ours
        // are disjoint. This is the arithmetic that has to hold between them.
        let maes = [0.0, 0.0, 0.4, 2.0, 2.5, 9.0, 40.0];
        let count = |v: Verdict| maes.iter().filter(|m| classify(**m) == v).count();
        let cumulative = |t: f64| maes.iter().filter(|m| **m <= t).count();
        assert_eq!(count(Verdict::Exact) + count(Verdict::Within1), cumulative(1.0));
        assert_eq!(
            count(Verdict::Exact) + count(Verdict::Within1) + count(Verdict::Within3),
            cumulative(3.0)
        );
        assert_eq!(
            count(Verdict::Exact)
                + count(Verdict::Within1)
                + count(Verdict::Within3)
                + count(Verdict::Within10),
            cumulative(10.0)
        );
    }

    #[test]
    fn the_verdict_keys_are_the_tally_keys() {
        for (v, k) in [
            (Verdict::Exact, "exact"),
            (Verdict::Within1, "within1"),
            (Verdict::Within3, "within3"),
            (Verdict::Within10, "within10"),
            (Verdict::Over, "over"),
            (Verdict::StructureOnly, "structureOnly"),
            (Verdict::Failed, "failed"),
        ] {
            assert_eq!(v.key(), k);
        }
        assert!(!Verdict::StructureOnly.is_comparison());
        assert!(!Verdict::Failed.is_comparison());
        assert!(Verdict::Over.is_comparison());
    }

    // --- the histogram ---------------------------------------------------

    #[test]
    fn every_error_lands_in_a_bucket_that_exists() {
        for mae in [0.0, 0.001, 0.01, 0.0138, 0.1, 1.0, 3.0, 10.0, 100.0, 255.0, 1e9] {
            assert!(bucket_of(mae) < HISTOGRAM_BUCKETS, "{mae} escaped the histogram");
        }
        assert_eq!(bucket_of(f64::NAN), 0, "a NaN must not index out of bounds");
        assert_eq!(bucket_of(-1.0), 0);
    }

    #[test]
    fn the_buckets_rise_with_the_error_and_the_top_one_catches_everything_worse() {
        let mut last = 0;
        for mae in [0.0, 0.02, 0.05, 0.2, 1.0, 4.0, 20.0, 120.0, 255.0] {
            let b = bucket_of(mae);
            assert!(b >= last, "bucket went backwards at {mae}");
            last = b;
        }
        assert_eq!(bucket_of(0.0), 0);
        assert_eq!(bucket_of(0.4), 0);
        assert_eq!(bucket_of(0.5), 1, "the boundary belongs to the upper bucket");
        assert_eq!(bucket_of(3.0), 6);
        // Everything past the top edge piles into the last bucket rather than
        // vanishing off the end of the chart.
        assert_eq!(bucket_of(15.5), HISTOGRAM_BUCKETS - 1);
        assert_eq!(bucket_of(255.0), HISTOGRAM_BUCKETS - 1);
        assert_eq!(bucket_of(f64::MAX), HISTOGRAM_BUCKETS - 1);
    }

    #[test]
    fn the_edges_agree_with_the_bucketing() {
        let edges = bucket_edges();
        assert_eq!(edges.len(), HISTOGRAM_BUCKETS + 1);
        assert_eq!(edges[0], 0.0);
        assert_eq!(*edges.last().unwrap(), 16.0);
        for w in edges.windows(2) {
            assert!(w[1] > w[0], "edges must rise: {w:?}");
        }
        // A value on a bucket's lower edge bucketes there.
        for (i, e) in edges.iter().enumerate().take(HISTOGRAM_BUCKETS).skip(1) {
            assert_eq!(bucket_of(*e), i, "edge {i} = {e}");
        }
    }

    // --- the aligner -----------------------------------------------------

    #[test]
    fn equal_frames_are_borrowed_not_copied() {
        let ours = solid(4, 4, [10, 20, 30, 255]);
        let theirs = solid(4, 4, [10, 20, 30, 255]);
        let a = align(Frame::new(4, 4, &ours), Some(Frame::new(4, 4, &theirs))).unwrap();
        assert!(!a.cropped);
        assert!(matches!(a.ours, Cow::Borrowed(_)));
        assert_eq!((a.width, a.height), (4, 4));
        assert_eq!(metrics(&a.ours, &a.theirs).mae, 0.0);
    }

    #[test]
    fn a_size_mismatch_is_cropped_to_the_overlap_and_says_so() {
        // Ours is 4x4, Photoshop's is 3x5. The overlap is 3x4, and the crop
        // has to respect the source stride or the rows shear.
        let mut ours = Vec::new();
        for y in 0..4u8 {
            for x in 0..4u8 {
                ours.extend_from_slice(&[x * 10, y * 10, 0, 255]);
            }
        }
        let mut theirs = Vec::new();
        for y in 0..5u8 {
            for x in 0..3u8 {
                theirs.extend_from_slice(&[x * 10, y * 10, 0, 255]);
            }
        }
        let a = align(Frame::new(4, 4, &ours), Some(Frame::new(3, 5, &theirs))).unwrap();
        assert!(a.cropped);
        assert_eq!((a.width, a.height), (3, 4));
        assert_eq!(a.ours.len(), 3 * 4 * 4);
        assert_eq!(a.theirs.len(), 3 * 4 * 4);
        // The overlap holds the same gradient in both, so it must measure 0.
        assert_eq!(metrics(&a.ours, &a.theirs).mae, 0.0);
    }

    #[test]
    fn a_file_with_no_composite_is_not_a_comparison() {
        let ours = solid(4, 4, [0, 0, 0, 255]);
        assert_eq!(
            align(Frame::new(4, 4, &ours), None).unwrap_err(),
            AlignError::NoComposite
        );
    }

    #[test]
    fn an_empty_or_short_frame_is_refused_rather_than_measured() {
        let ours = solid(4, 4, [0, 0, 0, 255]);
        let empty: [u8; 0] = [];
        assert_eq!(
            align(Frame::new(0, 0, &empty), Some(Frame::new(4, 4, &ours))).unwrap_err(),
            AlignError::Malformed
        );
        // A buffer that does not match its declared size would read out of
        // bounds during the crop.
        let short = solid(4, 2, [0, 0, 0, 255]);
        assert_eq!(
            align(Frame::new(4, 4, &ours), Some(Frame::new(4, 4, &short))).unwrap_err(),
            AlignError::Malformed
        );
    }

    // --- the measurement -------------------------------------------------

    #[test]
    fn the_mae_is_the_one_composite_diff_computes() {
        // One channel out by 30 on every pixel: 30/3 = 10 per pixel.
        let a = solid(2, 2, [100, 100, 100, 255]);
        let b = solid(2, 2, [130, 100, 100, 255]);
        let m = metrics(&a, &b);
        assert!((m.mae - 10.0).abs() < 1e-9, "{}", m.mae);
    }

    #[test]
    fn transparency_is_composited_over_white_before_it_is_measured() {
        // Fully transparent black and fully transparent white are the same
        // picture. Comparing the raw bytes would score them 255 apart.
        let clear_black = solid(2, 2, [0, 0, 0, 0]);
        let clear_white = solid(2, 2, [255, 255, 255, 0]);
        assert_eq!(metrics(&clear_black, &clear_white).mae, 0.0);
    }

    #[test]
    fn the_p95_ignores_the_worst_five_per_cent() {
        // 100 pixels: 96 identical, 4 wildly wrong. The mean moves; the 95th
        // percentile stays at zero because 95% of the pixels agree.
        let mut a = solid(10, 10, [0, 0, 0, 255]);
        let mut b = a.clone();
        for i in 0..4 {
            b[i * 4] = 255;
            b[i * 4 + 1] = 255;
            b[i * 4 + 2] = 255;
        }
        let m = metrics(&a, &b);
        assert!(m.mae > 0.0, "the mean must notice the four bad pixels");
        assert_eq!(m.p95, 0.0, "but the 95th percentile must not");
        // Push it to six bad pixels and the percentile has to move.
        for i in 0..6 {
            a[i * 4] = 0;
            b[i * 4] = 255;
            b[i * 4 + 1] = 255;
            b[i * 4 + 2] = 255;
        }
        assert!(metrics(&a, &b).p95 > 0.0);
    }

    #[test]
    fn nothing_to_measure_measures_zero_rather_than_dividing_by_zero() {
        assert_eq!(metrics(&[], &[]), Metrics { mae: 0.0, p95: 0.0 });
    }

    // --- the pictures ----------------------------------------------------

    #[test]
    fn a_tiny_file_is_magnified_rather_than_shrunk() {
        let rgb = vec![255u8; 4 * 4 * 3];
        let uri = jpeg_data_uri(4, 4, &rgb, THUMB_MAX_SIDE, THUMB_QUALITY).unwrap();
        assert!(uri.starts_with("data:image/jpeg;base64,"));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(uri.trim_start_matches("data:image/jpeg;base64,"))
            .unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        assert_eq!((img.width(), img.height()), (256, 256));
    }

    #[test]
    fn a_large_file_is_shrunk_to_the_long_side_keeping_its_shape() {
        let rgb = vec![128u8; 1000 * 500 * 3];
        let uri = jpeg_data_uri(1000, 500, &rgb, THUMB_MAX_SIDE, THUMB_QUALITY).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(uri.trim_start_matches("data:image/jpeg;base64,"))
            .unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        assert_eq!((img.width(), img.height()), (256, 128));
        assert!(bytes.len() < 30_000, "{} bytes is too big to stream", bytes.len());
    }

    #[test]
    fn the_difference_map_is_black_where_the_two_agree() {
        let a = solid(2, 2, [77, 88, 99, 255]);
        let d = rgb_diff(&a, &a);
        assert!(d.iter().all(|b| *b == 0));
        let b = solid(2, 2, [87, 88, 99, 255]);
        let d = rgb_diff(&a, &b);
        // 10 levels out, amplified by 4.
        assert_eq!(d[0], 40);
        assert_eq!(d[1], 0);
    }
}
