//! One-time tokens that hand a named file to the UI.
//!
//! `openphotoedit open photo.jpg` issues a token for that one path and opens
//! `/?open=<token>`. The page fetches `/api/file/<token>` once; after that, or
//! after [`TTL`], the token is dead. The path never appears in a URL, and there
//! is no way to ask for a path the person running the binary did not name.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long an unused token stays valid. Long enough to survive a slow first
/// launch and a browser that takes its time; short enough that a URL left in
/// history is worthless.
pub const TTL: Duration = Duration::from_secs(10 * 60);

/// How long a spent token is remembered, so a second fetch says "gone"
/// rather than "never existed".
const TOMBSTONE: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone)]
pub struct Handoff {
    pub path: PathBuf,
    /// The file name shown in the UI.
    pub name: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TakeError {
    /// Not a token this process issued.
    Unknown,
    /// Already used, or expired.
    Gone,
}

struct Entry {
    handoff: Option<Handoff>,
    issued: Instant,
}

#[derive(Default)]
pub struct Tokens {
    map: Mutex<HashMap<String, Entry>>,
}

impl Tokens {
    pub fn new() -> Tokens {
        Tokens::default()
    }

    /// Issue a token for `path`. 256 random bits, hex.
    pub fn issue(&self, path: PathBuf) -> String {
        let mut raw = [0u8; 32];
        getrandom::fill(&mut raw).expect("the operating system's random source is unavailable");
        let token: String = raw.iter().map(|b| format!("{b:02x}")).collect();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        let mut map = self.map.lock().unwrap();
        map.insert(token.clone(), Entry { handoff: Some(Handoff { path, name }), issued: Instant::now() });
        token
    }

    /// Spend a token. Succeeds at most once per token.
    pub fn take(&self, token: &str) -> Result<Handoff, TakeError> {
        self.take_at(token, Instant::now())
    }

    fn take_at(&self, token: &str, now: Instant) -> Result<Handoff, TakeError> {
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(TakeError::Unknown);
        }
        let mut map = self.map.lock().unwrap();
        map.retain(|_, e| now.duration_since(e.issued) < TTL + TOMBSTONE);
        let entry = map.get_mut(token).ok_or(TakeError::Unknown)?;
        if now.duration_since(entry.issued) >= TTL {
            entry.handoff = None;
        }
        entry.handoff.take().ok_or(TakeError::Gone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_use() {
        let t = Tokens::new();
        let tok = t.issue("/tmp/a.jpg".into());
        assert_eq!(tok.len(), 64);
        let h = t.take(&tok).unwrap();
        assert_eq!(h.name, "a.jpg");
        assert_eq!(t.take(&tok).unwrap_err(), TakeError::Gone);
        assert_eq!(t.take(&"0".repeat(64)).unwrap_err(), TakeError::Unknown);
        assert_eq!(t.take("../etc").unwrap_err(), TakeError::Unknown);
    }

    #[test]
    fn expires() {
        let t = Tokens::new();
        let tok = t.issue("/tmp/a.jpg".into());
        let later = Instant::now() + TTL + Duration::from_secs(1);
        assert_eq!(t.take_at(&tok, later).unwrap_err(), TakeError::Gone);
    }

    #[test]
    fn tokens_differ() {
        let t = Tokens::new();
        assert_ne!(t.issue("/a".into()), t.issue("/a".into()));
    }
}
