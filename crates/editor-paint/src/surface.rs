//! What a paint operation writes to: a layer's pixels or its mask. Both are
//! read and written as straight RGBA8 (a mask reads as opaque grey and is
//! written back from the red channel), so every tool works on either.
//!
//! The rules match `editor_core::pixels::edit_layer`: locked layers refuse,
//! text/shape/smart layers must be rasterized first, a transparency lock keeps
//! the layer's alpha, and rasters grow to cover what is painted.

use editor_core::document::Document;
use editor_core::geom::Rect;
use editor_core::layer::{LayerId, LayerKind};
use editor_core::ops::{EditorError, Result};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    #[default]
    Pixels,
    Mask,
}

pub fn check(doc: &Document, id: LayerId, target: Target) -> Result<()> {
    let l = doc.find(id).ok_or(EditorError::NoLayer(id))?;
    if l.locks.all {
        return Err(EditorError::Locked(l.name.clone()));
    }
    match target {
        Target::Mask => {
            if l.mask.is_none() {
                return Err(EditorError::Invalid(format!("`{}` has no layer mask to paint on", l.name)));
            }
            Ok(())
        }
        Target::Pixels => {
            if l.locks.pixels {
                return Err(EditorError::Locked(l.name.clone()));
            }
            match l.kind {
                LayerKind::Pixel(_) => Ok(()),
                LayerKind::Text { .. } | LayerKind::Shape { .. } | LayerKind::Smart { .. } => {
                    Err(EditorError::Invalid(format!("`{}` is a {} layer; rasterize it first", l.name, l.kind.name())))
                }
                _ => Err(EditorError::Invalid(format!("`{}` has no pixels to paint on", l.name))),
            }
        }
    }
}

/// RGBA over a document rectangle. Outside the layer's pixels a layer reads
/// transparent and a mask reads its fill.
pub fn read(doc: &Document, id: LayerId, target: Target, rect: Rect) -> Result<Vec<u8>> {
    let l = doc.find(id).ok_or(EditorError::NoLayer(id))?;
    match target {
        Target::Pixels => {
            let r = l.raster().ok_or_else(|| EditorError::Invalid(format!("`{}` has no pixels", l.name)))?;
            Ok(r.plane.read_vec(rect.translate(-r.x, -r.y)))
        }
        Target::Mask => {
            let m = l.mask.as_ref().ok_or_else(|| EditorError::Invalid(format!("`{}` has no layer mask", l.name)))?;
            let v = m.raster.plane.read_vec(rect.translate(-m.raster.x, -m.raster.y));
            Ok(v.iter().flat_map(|&g| [g, g, g, 255]).collect())
        }
    }
}

/// Write RGBA over a document rectangle.
pub fn write(doc: &mut Document, id: LayerId, target: Target, rect: Rect, data: &[u8]) -> Result<()> {
    if rect.is_empty() {
        return Ok(());
    }
    let l = doc.find_mut(id).ok_or(EditorError::NoLayer(id))?;
    match target {
        Target::Pixels => {
            let lock_alpha = l.locks.transparency;
            let LayerKind::Pixel(r) = &mut l.kind else {
                return Err(EditorError::Invalid(format!("`{}` has no pixels to paint on", l.name)));
            };
            r.ensure_covers(rect);
            let local = rect.translate(-r.x, -r.y);
            if lock_alpha {
                let mut out = data.to_vec();
                let cur = r.plane.read_vec(local);
                for (o, c) in out.chunks_exact_mut(4).zip(cur.chunks_exact(4)) {
                    o[3] = c[3];
                }
                r.plane.write(local, &out);
            } else {
                r.plane.write(local, data);
            }
            r.plane.compact();
        }
        Target::Mask => {
            let m = l.mask.as_mut().ok_or_else(|| EditorError::Invalid(format!("`{}` has no layer mask", l.name)))?;
            m.raster.ensure_covers(rect);
            let local = rect.translate(-m.raster.x, -m.raster.y);
            let grey: Vec<u8> = data.chunks_exact(4).map(|p| p[0]).collect();
            m.raster.plane.write(local, &grey);
            m.raster.plane.compact();
        }
    }
    Ok(())
}

/// Identity of the target's storage, to notice when something other than
/// the current stroke changed it (an undo, another command).
pub fn signature(doc: &Document, id: LayerId, target: Target) -> Vec<usize> {
    let Some(l) = doc.find(id) else { return vec![usize::MAX] };
    let r = match target {
        Target::Pixels => l.raster(),
        Target::Mask => l.mask.as_ref().map(|m| &m.raster),
    };
    match r {
        None => vec![usize::MAX - 1],
        Some(r) => {
            let mut v = vec![r.x as usize, r.y as usize, r.plane.width() as usize, r.plane.height() as usize];
            v.extend(r.plane.tile_ptrs());
            v
        }
    }
}
