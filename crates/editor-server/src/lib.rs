//! OpenPhotoshop as one local program.
//!
//! The editor is a web app whose engine is Rust compiled to WebAssembly. This
//! crate is the same product as a single native binary: it serves the built
//! web app, the AI model files and a couple of small APIs on one loopback
//! origin, and it can run the engine natively (`openphotoshop render`).
//!
//! # One origin, on purpose
//!
//! The UI and everything it loads come from `http://127.0.0.1:<port>`. That
//! sidesteps mixed content, CORS and Private Network Access, which each broke
//! the hosted-page-plus-local-helper shape in OpenDownloader. It also makes
//! cross-origin isolation (COOP/COEP, needed for onnxruntime threads) a matter
//! of two headers on every response.
//!
//! # What it will not do
//!
//! There is no "read this path" API. A page on this origin, or a hostile page
//! that DNS-rebinds a name to 127.0.0.1, must not be able to read the disk.
//! Files reach the UI only when the person running the binary names them on
//! the command line (`openphotoshop open <file>`), through a one-time token.

pub mod http;
pub mod models;
pub mod paths;
pub mod render;
pub mod server;
pub mod tokens;
pub mod web;

pub use server::{router, AppState};

/// The product name as it appears in `/api/health` and in logs.
pub const APP: &str = "openphotoshop";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
