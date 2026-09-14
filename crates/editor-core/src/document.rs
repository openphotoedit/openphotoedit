//! The document: canvas size, the layer tree, the selection.

use serde::{Deserialize, Serialize};

use crate::layer::{Layer, LayerId, LayerKind};
use crate::plane::Plane;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocMeta {
    /// ICC profile bytes of the source file, kept so export can embed it.
    #[serde(skip)]
    pub icc: Option<Vec<u8>>,
    pub source_name: Option<String>,
    /// Ordered actions for Content Credentials: `(action, detail)`.
    pub actions: Vec<(String, String)>,
    /// Opaque data a file-format crate keeps with the document, keyed by
    /// format (`"psd"`: blocks this engine does not model, re-emitted on
    /// export). Cheap to clone; the project format saves it verbatim.
    #[serde(skip)]
    pub extra: std::collections::BTreeMap<String, Sidecar>,
}

/// Shared, immutable bytes: history snapshots clone the `Arc`, not the data.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Sidecar(pub std::sync::Arc<Vec<u8>>);

impl std::fmt::Debug for Sidecar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sidecar({} bytes)", self.0.len())
    }
}

#[derive(Clone, Debug)]
pub struct Guide {
    pub vertical: bool,
    pub position: f64,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    /// Pixels per inch.
    pub resolution: f32,
    /// Bottom to top, as PSD stores them.
    pub layers: Vec<Layer>,
    /// Single-channel; absent means "everything".
    pub selection: Option<Plane>,
    pub active: Option<LayerId>,
    pub guides: Vec<Guide>,
    pub meta: DocMeta,
    next_id: LayerId,
}

/// Where a layer lives: its parent group (None = top level) and its index
/// among that parent's children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub parent: Option<LayerId>,
    pub index: usize,
}

impl Document {
    pub fn new(width: u32, height: u32) -> Document {
        Document {
            width,
            height,
            resolution: 72.0,
            layers: Vec::new(),
            selection: None,
            active: None,
            guides: Vec::new(),
            meta: DocMeta::default(),
            next_id: 1,
        }
    }

    pub fn alloc_id(&mut self) -> LayerId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Make sure future ids do not collide with ones already present (after
    /// loading a file that carries its own ids).
    pub fn reserve_ids(&mut self) {
        let mut max = 0;
        self.walk(&mut |l| max = max.max(l.id));
        self.next_id = self.next_id.max(max + 1);
    }

    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a Layer)) {
        fn go<'a>(layers: &'a [Layer], f: &mut impl FnMut(&'a Layer)) {
            for l in layers {
                f(l);
                if let Some(c) = l.children() {
                    go(c, f);
                }
            }
        }
        go(&self.layers, f);
    }

    pub fn find(&self, id: LayerId) -> Option<&Layer> {
        fn go(layers: &[Layer], id: LayerId) -> Option<&Layer> {
            for l in layers {
                if l.id == id {
                    return Some(l);
                }
                if let Some(c) = l.children() {
                    if let Some(f) = go(c, id) {
                        return Some(f);
                    }
                }
            }
            None
        }
        go(&self.layers, id)
    }

    /// Index path from the top-level list down to the layer.
    pub fn path_of(&self, id: LayerId) -> Option<Vec<usize>> {
        fn go(layers: &[Layer], id: LayerId, path: &mut Vec<usize>) -> bool {
            for (i, l) in layers.iter().enumerate() {
                path.push(i);
                if l.id == id {
                    return true;
                }
                if let Some(c) = l.children() {
                    if go(c, id, path) {
                        return true;
                    }
                }
                path.pop();
            }
            false
        }
        let mut path = Vec::new();
        go(&self.layers, id, &mut path).then_some(path)
    }

    /// Mutable access. Bumps the revision of the layer and every ancestor,
    /// since any mutable access may change what renders.
    pub fn find_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        let path = self.path_of(id)?;
        let mut list: &mut Vec<Layer> = &mut self.layers;
        for (depth, &i) in path.iter().enumerate() {
            let layer = &mut list[i];
            layer.touch();
            if depth + 1 == path.len() {
                return Some(layer);
            }
            list = match &mut layer.kind {
                LayerKind::Group { children, .. } => children,
                _ => return None,
            };
        }
        None
    }

    pub fn slot_of(&self, id: LayerId) -> Option<Slot> {
        fn go(layers: &[Layer], parent: Option<LayerId>, id: LayerId) -> Option<Slot> {
            for (i, l) in layers.iter().enumerate() {
                if l.id == id {
                    return Some(Slot { parent, index: i });
                }
                if let Some(c) = l.children() {
                    if let Some(s) = go(c, Some(l.id), id) {
                        return Some(s);
                    }
                }
            }
            None
        }
        go(&self.layers, None, id)
    }

    /// The list a slot's parent owns.
    pub fn siblings_mut(&mut self, parent: Option<LayerId>) -> Option<&mut Vec<Layer>> {
        match parent {
            None => Some(&mut self.layers),
            Some(pid) => self.find_mut(pid).and_then(|p| p.children_mut()),
        }
    }

    pub fn siblings(&self, parent: Option<LayerId>) -> Option<&Vec<Layer>> {
        match parent {
            None => Some(&self.layers),
            Some(pid) => self.find(pid).and_then(|p| p.children()),
        }
    }

    pub fn remove(&mut self, id: LayerId) -> Option<Layer> {
        let slot = self.slot_of(id)?;
        let list = self.siblings_mut(slot.parent)?;
        let layer = list.remove(slot.index);
        if self.active == Some(id) {
            self.active = None;
        }
        Some(layer)
    }

    /// Insert at `index` within `parent` (clamped).
    pub fn insert(&mut self, parent: Option<LayerId>, index: usize, layer: Layer) -> bool {
        let Some(list) = self.siblings_mut(parent) else { return false };
        let i = index.min(list.len());
        list.insert(i, layer);
        true
    }

    /// Insert directly above `anchor` (same parent), or at the top of the
    /// document when there is no anchor.
    pub fn insert_above(&mut self, anchor: Option<LayerId>, layer: Layer) {
        match anchor.and_then(|a| self.slot_of(a)) {
            Some(slot) => {
                self.insert(slot.parent, slot.index + 1, layer);
            }
            None => self.layers.push(layer),
        }
    }

    pub fn is_ancestor(&self, ancestor: LayerId, id: LayerId) -> bool {
        let mut cur = self.slot_of(id).and_then(|s| s.parent);
        while let Some(p) = cur {
            if p == ancestor {
                return true;
            }
            cur = self.slot_of(p).and_then(|s| s.parent);
        }
        false
    }

    pub fn layer_count(&self) -> usize {
        let mut n = 0;
        self.walk(&mut |_| n += 1);
        n
    }

    /// Bytes of distinct tiles across the document.
    pub fn unique_tile_bytes(&self, seen: &mut std::collections::HashSet<usize>) -> usize {
        let mut bytes = 0;
        let mut count = |p: &Plane| {
            let per = (crate::plane::TILE * crate::plane::TILE) as usize * p.channels();
            for ptr in p.tile_ptrs() {
                if seen.insert(ptr) {
                    bytes += per;
                }
            }
        };
        self.walk(&mut |l| {
            if let Some(r) = l.raster() {
                count(&r.plane);
            }
            if let Some(m) = &l.mask {
                count(&m.raster.plane);
            }
        });
        if let Some(s) = &self.selection {
            count(s);
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::Raster;

    fn px(doc: &mut Document, name: &str) -> Layer {
        let id = doc.alloc_id();
        Layer::new(id, name, LayerKind::Pixel(Raster::empty(10, 10)))
    }

    #[test]
    fn tree_insert_find_remove() {
        let mut doc = Document::new(10, 10);
        let a = px(&mut doc, "a");
        let a_id = a.id;
        doc.layers.push(a);
        let gid = doc.alloc_id();
        let b = px(&mut doc, "b");
        let b_id = b.id;
        doc.layers.push(Layer::new(gid, "g", LayerKind::Group { children: vec![b], pass_through: true, expanded: true }));
        assert_eq!(doc.slot_of(b_id), Some(Slot { parent: Some(gid), index: 0 }));
        assert!(doc.is_ancestor(gid, b_id));
        let before = doc.find(gid).unwrap().rev;
        doc.find_mut(b_id).unwrap().name = "bb".into();
        assert!(doc.find(gid).unwrap().rev > before, "ancestor revision bumps");
        assert_eq!(doc.find(b_id).unwrap().name, "bb");
        let c = px(&mut doc, "c");
        doc.insert_above(Some(a_id), c);
        assert_eq!(doc.layers[1].name, "c");
        assert!(doc.remove(b_id).is_some());
        assert_eq!(doc.layer_count(), 3);
    }
}
