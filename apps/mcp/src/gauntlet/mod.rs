//! The Photoshop gauntlet: our renderer against Adobe's own output, over the
//! whole of `testdata/psd`, recomputed live.
//!
//! ```text
//! corpus::walk ─▶ run::measure ─▶ run::Progress ─▶ agui:: STATE_SNAPSHOT / STATE_DELTA
//!                    │
//!                    ├─ editor_psd::import          the layer tree
//!                    ├─ editor_core::render::flatten  our composite
//!                    └─ editor_psd::merged_rgba     the composite Photoshop wrote
//! ```
//!
//! The honest part matters more than the fast part. Every file's numbers come
//! from a render performed moments earlier; there is no recorded table in this
//! code path, nothing is rounded towards a pass, and the files we get wrong
//! are streamed with the same three pictures as the ones we get right.
//!
//! [`crate::agui`] is the transport. This module is the measurement.

pub mod compare;
pub mod corpus;
pub mod run;

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// What one run of the gauntlet was asked to do.
#[derive(Clone, Debug)]
pub struct RunOptions {
    /// The corpus root to walk.
    pub corpus: PathBuf,
    /// Keep only the files whose relative path contains this (comma-separated
    /// alternatives allowed). `None` runs everything.
    pub filter: Option<String>,
    /// A deliberate pause after each file so a person can watch. 0 is full
    /// speed.
    pub pace_ms: u64,
}

/// A pace that a human can follow without the run taking all afternoon:
/// 269 files at 120 ms is a little over half a minute.
pub const DEFAULT_PACE_MS: u64 = 120;

/// The largest pace we will accept, so a stray `?pace=99999999` cannot pin a
/// connection open for a week.
pub const MAX_PACE_MS: u64 = 10_000;

// --- what the MCP tool needs to know -------------------------------------

/// Where the demo is being served, once `--agui-port` has started it.
#[derive(Clone, Debug)]
pub struct Launch {
    pub port: u16,
    pub corpus: PathBuf,
    pub ui_dir: PathBuf,
    pub files: usize,
}

impl Launch {
    /// The page a person opens.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }

    /// The AG-UI endpoint a client posts to.
    pub fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}/agui", self.port)
    }
}

static LAUNCH: OnceLock<Launch> = OnceLock::new();
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// Record where the demo is. Called once, by `main`.
pub fn set_launch(launch: Launch) {
    let _ = LAUNCH.set(launch);
}

/// Where the demo is, or `None` when the server was started without
/// `--agui-port`.
pub fn launch() -> Option<&'static Launch> {
    LAUNCH.get()
}

/// How many runs are streaming right now. A second run does not disturb the
/// first: each connection walks the corpus with its own [`run::Progress`], and
/// the corpus is only ever read.
pub fn active_runs() -> usize {
    ACTIVE.load(Ordering::Relaxed)
}

/// Counts one live run for as long as it is held. Lives inside the agent, so
/// it drops when the response body drops — which is also when the run is
/// cancelled.
pub struct ActiveRun;

impl ActiveRun {
    pub fn begin() -> ActiveRun {
        ACTIVE.fetch_add(1, Ordering::Relaxed);
        ActiveRun
    }
}

impl Drop for ActiveRun {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_counts_while_it_lives_and_not_after() {
        let before = active_runs();
        {
            let _a = ActiveRun::begin();
            let _b = ActiveRun::begin();
            assert_eq!(active_runs(), before + 2);
        }
        assert_eq!(active_runs(), before);
    }

    #[test]
    fn the_urls_are_loopback_only() {
        let l = Launch {
            port: 5261,
            corpus: PathBuf::from("/tmp/psd"),
            ui_dir: PathBuf::from("/tmp/ui"),
            files: 269,
        };
        assert_eq!(l.url(), "http://127.0.0.1:5261/");
        assert_eq!(l.endpoint(), "http://127.0.0.1:5261/agui");
    }
}
