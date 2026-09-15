//! Reading the pixels a selection algorithm looks at: one layer's own pixels
//! or the visible composite, loaded lazily in canvas tiles so a contiguous
//! flood fill over a small region never renders the whole document.

use editor_core::document::Document;
use editor_core::geom::Rect;
use editor_core::layer::{LayerId, Raster};
use editor_core::pixels::read_merged;

const SHIFT: i32 = 8;
const TILE: i32 = 1 << SHIFT;

pub struct Sampler<'a> {
    doc: &'a Document,
    raster: Option<&'a Raster>,
    pub width: i32,
    pub height: i32,
    cols: i32,
    tiles: Vec<Option<Box<[u8]>>>,
}

impl<'a> Sampler<'a> {
    /// Sample `layer`'s own pixels, or the composite when `sample_all` is set
    /// or the layer has no pixels of its own (adjustment, fill, group).
    pub fn new(doc: &'a Document, layer: Option<LayerId>, sample_all: bool) -> Sampler<'a> {
        let raster = if sample_all { None } else { layer.and_then(|id| doc.find(id)).and_then(|l| l.raster()) };
        let (width, height) = (doc.width as i32, doc.height as i32);
        let cols = (width + TILE - 1) >> SHIFT;
        let rows = (height + TILE - 1) >> SHIFT;
        Sampler { doc, raster, width, height, cols, tiles: vec![None; (cols.max(1) * rows.max(1)) as usize] }
    }

    pub fn merged(doc: &'a Document) -> Sampler<'a> {
        Sampler::new(doc, None, true)
    }

    pub fn canvas(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    /// RGBA of a canvas rectangle (not cached; for whole-image passes).
    pub fn read(&self, rect: Rect) -> Vec<u8> {
        match self.raster {
            Some(r) => r.plane.read_vec(rect.translate(-r.x, -r.y)),
            None => read_merged(self.doc, rect),
        }
    }

    #[cold]
    fn load(&mut self, tx: i32, ty: i32) {
        let r = Rect::new(tx << SHIFT, ty << SHIFT, TILE, TILE).intersect(&self.canvas());
        let data = self.read(r).into_boxed_slice();
        self.tiles[(ty * self.cols + tx) as usize] = Some(data);
    }

    /// One canvas pixel; `(x, y)` must lie inside the canvas.
    #[inline]
    pub fn get(&mut self, x: i32, y: i32) -> [u8; 4] {
        let (tx, ty) = (x >> SHIFT, y >> SHIFT);
        let i = (ty * self.cols + tx) as usize;
        if self.tiles[i].is_none() {
            self.load(tx, ty);
        }
        let t = self.tiles[i].as_deref().unwrap_or(&[]);
        let tw = (self.width - (tx << SHIFT)).min(TILE);
        let k = (((y - (ty << SHIFT)) * tw + (x - (tx << SHIFT))) * 4) as usize;
        [t[k], t[k + 1], t[k + 2], t[k + 3]]
    }
}
