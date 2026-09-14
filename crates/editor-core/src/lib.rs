//! OpenPhotoshop's editing engine.
//!
//! Pure Rust with no I/O: the same code runs as WebAssembly in a browser
//! worker and natively in the local server. A document is a tree of layers
//! over sparse copy-on-write tiles; every edit is a JSON command; every
//! command is an undo step; one CPU compositor renders the viewport, the
//! brush patch and the export.

pub mod adjust;
pub mod blend;
pub mod color;
pub mod document;
pub mod editor;
pub mod geom;
pub mod history;
pub mod layer;
pub mod ops;
pub mod plane;
pub mod render;
pub mod selection;
pub mod shape;
pub mod summary;
pub mod transform;

pub use document::Document;
pub use editor::Editor;
pub use geom::{Point, Rect};
pub use layer::{Layer, LayerId, LayerKind};
pub use plane::Plane;
pub use render::View;
