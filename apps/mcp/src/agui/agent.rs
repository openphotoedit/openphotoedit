//! The gauntlet as an AG-UI agent.
//!
//! One run is one walk of the corpus. The events, in the order the protocol
//! requires:
//!
//! ```text
//! RUN_STARTED
//!   TEXT_MESSAGE_START / CONTENT / END      what this run is about to do
//!   STATE_SNAPSHOT                          the whole state, once
//!   for each file:
//!     STEP_STARTED  <relative path>
//!     STATE_DELTA   [/index, /phase=import]
//!     STATE_DELTA   [/phase=flatten]
//!     STATE_DELTA   [/phase=compare]
//!     STATE_DELTA   the result: /current, /tally/…, /histogram/n, /worst, /throughput
//!     STEP_FINISHED <relative path>
//!   STATE_DELTA     [/phase=done, /throughput]
//!   TEXT_MESSAGE_START / CONTENT / END      the tally
//! RUN_FINISHED  result = the summary
//! ```
//!
//! The three phase deltas are two operations each and cost nothing; they are
//! what lets a viewer see that a 40 MB file is being imported rather than
//! that the demo has hung. The one that carries the pictures is the result
//! delta, one per file, exactly as the front end expects.
//!
//! `STATE_DELTA` and not a fresh snapshot per file because the snapshot holds
//! the five worst files' thumbnails as well as the current one's: resending it
//! 269 times would be about 60 MB down a loopback socket for no reason.

use ag_ui::server::{Agent, RunContext};
use ag_ui::{Event, PatchOperation, RunOutcome};

use crate::gauntlet::run::{self, FileOutcome, Phase, Progress, Replace};
use crate::gauntlet::{compare::Verdict, corpus, ActiveRun, RunOptions};

/// One run of the gauntlet, served over AG-UI.
pub struct GauntletAgent {
    opts: RunOptions,
    /// Counted for `start_gauntlet`, and dropped with the response body — so
    /// a browser tab that closes stops being a run in progress.
    _active: ActiveRun,
}

impl GauntletAgent {
    pub fn new(opts: RunOptions) -> GauntletAgent {
        GauntletAgent {
            opts,
            _active: ActiveRun::begin(),
        }
    }
}

fn patch(ops: Vec<Replace>) -> Vec<PatchOperation> {
    ops.into_iter()
        .map(|(path, value)| PatchOperation::Replace { path, value })
        .collect()
}

impl Agent for GauntletAgent {
    // The run's own state is published by hand as SNAPSHOT + DELTA, so the
    // framework has no state of its own to carry.
    type State = ();

    async fn run(&self, ctx: &mut RunContext<()>) -> ag_ui::server::Result<RunOutcome> {
        let entries = corpus::select(
            corpus::walk(&self.opts.corpus),
            self.opts.filter.as_deref(),
        );
        let mut progress = Progress::new(entries.len());

        ctx.say(format!(
            "The Photoshop gauntlet: {} file{} from {}, rendered here and compared \
             against the composite Photoshop saved inside each one.{}",
            entries.len(),
            if entries.len() == 1 { "" } else { "s" },
            self.opts.corpus.display(),
            match self.opts.filter.as_deref() {
                Some(f) => format!(" Filtered to {f:?}."),
                None => String::new(),
            }
        ))?;
        ctx.emit(Event::state_snapshot(progress.snapshot(Phase::Import, None)))?;

        for entry in &entries {
            ctx.check_cancelled()?;
            ctx.emit(Event::step_started(entry.rel.clone()))?;

            // The measurement is CPU-bound and takes long enough on a large
            // file to stall the reactor, so it runs on the blocking pool and
            // reports its phase back over a channel. Without that the stream
            // would go quiet for the whole of a slow file.
            let index = progress.index + 1;
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Phase>();
            let job = entry.clone();
            let mut task = tokio::task::spawn_blocking(move || {
                run::measure(&job, |p| {
                    let _ = tx.send(p);
                })
            });
            let finished = loop {
                tokio::select! {
                    Some(phase) = rx.recv() => {
                        ctx.emit(Event::state_delta(patch(run::phase_ops(index, phase))))?;
                    }
                    joined = &mut task => break joined,
                }
            };
            let outcome = match finished {
                Ok(o) => o,
                // A panic in the importer is a result, not a reason to stop —
                // and it is certainly not a pass.
                Err(e) => FileOutcome::blank(
                    &entry.rel,
                    Verdict::Failed,
                    format!("the worker did not finish: {e}"),
                    0,
                ),
            };

            let changed = progress.record(&outcome);
            let phase = if progress.index == entries.len() { Phase::Done } else { Phase::Compare };
            ctx.emit(Event::state_delta(patch(run::delta_ops(
                &progress, phase, &outcome, &changed,
            ))))?;
            ctx.emit(Event::step_finished(entry.rel.clone()))?;

            if self.opts.pace_ms > 0 {
                let nap = tokio::time::sleep(std::time::Duration::from_millis(self.opts.pace_ms));
                if ctx.until_cancelled(nap).await.is_none() {
                    return Err(ag_ui::server::Error::Cancelled);
                }
            }
        }

        ctx.emit(Event::state_delta(patch(vec![
            ("/phase".to_string(), serde_json::json!(Phase::Done.key())),
            ("/throughput".to_string(), progress.throughput_json()),
        ])))?;

        let t = progress.tally;
        ctx.say(format!(
            "{} files in {:.1} s. Exact {}, within 1 level {}, within 3 {}, within 10 {}, \
             over 10 {}. {} had no composite to compare against and {} failed outright.",
            progress.index,
            progress.elapsed_ms() as f64 / 1000.0,
            t.exact,
            t.within1,
            t.within3,
            t.within10,
            t.over,
            t.structure_only,
            t.failed,
        ))?;

        // Emitted by hand rather than left to the driver, so the terminal
        // event can carry the tally as its `result`. The driver notices the
        // run is already terminated and does not send a second one.
        let (thread, run) = (ctx.thread_id().clone(), ctx.run_id().clone());
        ctx.emit(
            ag_ui::event::RunFinishedEvent::new(thread, run)
                .with_outcome(RunOutcome::Success)
                .with_result(progress.summary())
                .into(),
        )
        .ok();
        Ok(RunOutcome::Success)
    }
}
