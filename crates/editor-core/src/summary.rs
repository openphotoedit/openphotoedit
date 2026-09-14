//! The JSON the UI's panels are built from: the layer tree without pixels,
//! the selection bounds, and the history.

use serde_json::{json, Value};

use crate::editor::Editor;
use crate::layer::{Layer, LayerKind};
use crate::selection;

pub fn layer(l: &Layer) -> Value {
    let mut v = json!({
        "id": l.id,
        "name": l.name,
        "kind": l.kind.name(),
        "visible": l.visible,
        "opacity": l.opacity,
        "fillOpacity": l.fill_opacity,
        "blend": l.blend,
        "clip": l.clip,
        "locks": l.locks,
        "colorLabel": l.color_label,
        "rev": l.rev,
        "provenance": l.provenance,
        "mask": l.mask.as_ref().map(|m| json!({ "enabled": m.enabled, "linked": m.linked, "density": m.density })),
    });
    let o = v.as_object_mut().unwrap();
    match &l.kind {
        LayerKind::Pixel(r) => {
            o.insert("bounds".into(), json!(r.doc_rect()));
        }
        LayerKind::Adjustment(a) => {
            o.insert("adjustment".into(), serde_json::to_value(a).unwrap_or(Value::Null));
        }
        LayerKind::Fill(f) => {
            o.insert("fill".into(), serde_json::to_value(f).unwrap_or(Value::Null));
        }
        LayerKind::Group { children, pass_through, expanded } => {
            o.insert("passThrough".into(), json!(pass_through));
            o.insert("expanded".into(), json!(expanded));
            o.insert("children".into(), Value::Array(children.iter().map(layer).collect()));
        }
        LayerKind::Text { data, raster } => {
            o.insert("text".into(), serde_json::to_value(data).unwrap_or(Value::Null));
            o.insert("bounds".into(), json!(raster.doc_rect()));
        }
        LayerKind::Shape { data, raster } => {
            o.insert("shape".into(), serde_json::to_value(data).unwrap_or(Value::Null));
            o.insert("bounds".into(), json!(raster.doc_rect()));
        }
    }
    v
}

pub fn document(ed: &Editor) -> Value {
    let doc = &ed.doc;
    json!({
        "width": doc.width,
        "height": doc.height,
        "resolution": doc.resolution,
        "active": doc.active,
        "layers": doc.layers.iter().map(layer).collect::<Vec<_>>(),
        "selection": doc.selection.as_ref().map(|_| json!({ "bounds": selection::bounds(doc) })),
        "history": {
            "undo": ed.history.undo_labels(),
            "redo": ed.history.redo_labels(),
        },
        "revision": ed.revision,
        "source": doc.meta.source_name,
    })
}
