//! Turning a URL path into a file path without leaving the folder.
//!
//! Two layers, because each catches what the other cannot:
//!
//! 1. [`clean`] works on the text. It percent-decodes once and rejects any
//!    segment that could climb (`..`), name a drive or stream (`C:`, `a:b`),
//!    smuggle a separator (`\`), or hide (`.git`, `.env`). Encoded forms such
//!    as `%2e%2e` and `..%2f` decode into the same rejected shapes. A second
//!    round of encoding (`%252e`) decodes to a literal `%2e` file name, which
//!    is harmless because it is never decoded again.
//! 2. [`resolve_under`] works on the file system. After joining, the result is
//!    canonicalised and must still sit under the canonical base, so a symlink
//!    inside the folder cannot point the server at the rest of the disk.

use std::path::{Path, PathBuf};

use percent_encoding::percent_decode_str;

/// Why a path was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadPath {
    /// Not valid percent-encoded UTF-8.
    Encoding,
    /// A segment that could escape the folder or reach something hidden.
    Forbidden,
}

/// Percent-decode `raw` (a URL path relative to some prefix, no leading `/`
/// required) into safe segments. Empty segments are dropped, so `a//b` is
/// `a/b` and a trailing `/` is ignored.
pub fn clean(raw: &str) -> Result<Vec<String>, BadPath> {
    let decoded = percent_decode_str(raw).decode_utf8().map_err(|_| BadPath::Encoding)?;
    let mut out = Vec::new();
    for seg in decoded.split('/') {
        if seg.is_empty() {
            continue;
        }
        let forbidden = seg == "."
            || seg == ".."
            || seg.starts_with('.')
            || seg.contains('\\')
            || seg.contains(':')
            || seg.chars().any(|c| c.is_control());
        if forbidden {
            return Err(BadPath::Forbidden);
        }
        out.push(seg.to_string());
    }
    Ok(out)
}

/// `segments` joined with `/`, the form rust-embed keys use.
pub fn join_key(segments: &[String]) -> String {
    segments.join("/")
}

/// The existing regular file at `base/segments…`, if it is really inside
/// `base` after following symlinks.
pub fn resolve_under(base: &Path, segments: &[String]) -> Option<PathBuf> {
    if segments.is_empty() {
        return None;
    }
    let mut p = base.to_path_buf();
    for s in segments {
        p.push(s);
    }
    let canon = p.canonicalize().ok()?;
    let canon_base = base.canonicalize().ok()?;
    if !canon.starts_with(&canon_base) || !canon.is_file() {
        return None;
    }
    Some(canon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_paths() {
        assert_eq!(clean("assets/index-abc.js").unwrap(), vec!["assets", "index-abc.js"]);
        assert_eq!(clean("a//b/").unwrap(), vec!["a", "b"]);
        assert_eq!(clean("my%20model.onnx").unwrap(), vec!["my model.onnx"]);
    }

    #[test]
    fn rejects_climbing_in_every_spelling() {
        for raw in [
            "../etc/passwd",
            "a/../../etc/passwd",
            "%2e%2e/etc/passwd",
            "%2E%2E%2Fetc%2Fpasswd",
            "..%2f..%2fetc%2fpasswd",
            "..%5c..%5cwindows",
            "a\\..\\b",
            "C:/Windows/win.ini",
            "file.onnx:stream",
            ".git/config",
            "ok/%00",
            "./x",
        ] {
            assert!(clean(raw).is_err(), "{raw} should be refused");
        }
        assert_eq!(clean("%ff").unwrap_err(), BadPath::Encoding);
    }

    #[test]
    fn double_encoding_stays_literal() {
        assert_eq!(clean("%252e%252e/x").unwrap(), vec!["%2e%2e", "x"]);
    }

    #[test]
    fn symlinks_cannot_leave_the_base() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), b"s").unwrap();
        std::fs::write(dir.path().join("inside.txt"), b"i").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), dir.path().join("link.txt")).unwrap();
        assert!(resolve_under(dir.path(), &["inside.txt".into()]).is_some());
        #[cfg(unix)]
        assert!(resolve_under(dir.path(), &["link.txt".into()]).is_none());
        assert!(resolve_under(dir.path(), &["missing".into()]).is_none());
    }
}
