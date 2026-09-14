//! `ai.*` commands.

use serde::Deserialize;

use super::{Applied, EditorError, Result};
use crate::document::Document;

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
pub enum Cmd {}

pub fn apply(cmd: Cmd, _doc: &mut Document, _bytes: &[u8]) -> Result<Applied> {
    match cmd {}
}

#[allow(dead_code)]
fn _unused() -> EditorError {
    EditorError::Invalid(String::new())
}
