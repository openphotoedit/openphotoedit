//! Start the real binary with `--agui-port`, run a handful of real corpus
//! files through it, and check the stream a browser would receive.
//!
//! The unit tests check the arithmetic. This checks the thing the demo
//! actually is: a process, a socket, `text/event-stream`, and an event
//! sequence that an AG-UI client is entitled to assume. Everything it asserts
//! is something the front end would break on — the opening event, the closing
//! event, matched steps, the state shape, and a tally that adds up to the
//! files that went past.
//!
//! It runs against `testdata/psd` itself rather than a fixture, because a
//! fidelity demo that has never seen a Photoshop file has not been tested.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

const BINARY: &str = env!("CARGO_BIN_EXE_openphotoedit-mcp");

/// Four real files, chosen to exercise the branches: a plain layered file, a
/// group, a file with layer effects (which we get visibly wrong), and a 32-bit
/// document that carries no usable composite at all.
const FILTER: &str = "ag-psd/layers.psd,ag-psd/groups.psd,ag-psd/effects.psd,ag-psd/32bits.psd";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("apps/mcp lives in the repository")
}

/// A server on a port of its own, killed when the test ends however it ends.
struct Server {
    child: Child,
    port: u16,
}

impl Server {
    fn start() -> Server {
        // `--agui-port 0` lets the OS pick, so two of these can run at once and
        // neither has to guess at a free port. The chosen one comes back on
        // stderr.
        let mut child = Command::new(BINARY)
            .args(["--root", repo_root().to_str().unwrap()])
            .args(["--agui-port", "0", "--no-stdio"])
            .args(["--gauntlet-corpus", repo_root().join("testdata/psd").to_str().unwrap()])
            .args(["--pace", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the server binary should start");

        let stderr = BufReader::new(child.stderr.take().unwrap());
        let mut port = None;
        for line in stderr.lines().map_while(Result::ok) {
            if let Some(rest) = line.strip_prefix("gauntlet ready: http://127.0.0.1:") {
                port = rest.split('/').next().and_then(|p| p.parse().ok());
                break;
            }
        }
        let port = port.expect("the server should announce the port it bound");
        Server { child, port }
    }

    /// Issue a GET and read the whole response body. The gauntlet closes the
    /// stream when the run ends, so a plain read to EOF is the whole run.
    fn get(&self, path: &str) -> (String, String) {
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut socket = loop {
            match TcpStream::connect(("127.0.0.1", self.port)) {
                Ok(s) => break s,
                Err(e) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                    let _ = e;
                }
                Err(e) => panic!("could not reach the gauntlet: {e}"),
            }
        };
        socket.set_read_timeout(Some(Duration::from_secs(180))).unwrap();
        write!(
            socket,
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: text/event-stream\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut raw = Vec::new();
        socket.read_to_end(&mut raw).unwrap();
        let text = String::from_utf8_lossy(&raw).into_owned();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let body = if head.to_lowercase().contains("transfer-encoding: chunked") {
            dechunk(body)
        } else {
            body.to_string()
        };
        (head.to_string(), body)
    }
}

/// Undo HTTP/1.1 chunked transfer encoding.
///
/// hyper chunks the SSE body, and a chunk boundary can fall anywhere — in the
/// middle of a base64 JPEG, or between the two newlines that end an SSE frame.
/// Reading the raw socket without undoing this is how you end up asserting
/// against a hex length prefix.
fn dechunk(mut body: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = body.split_once("\r\n") {
        let size = usize::from_str_radix(size.split(';').next().unwrap_or("").trim(), 16)
            .unwrap_or_else(|e| panic!("bad chunk size {size:?}: {e}"));
        if size == 0 {
            break;
        }
        assert!(rest.len() >= size, "a chunk was truncated");
        out.push_str(&rest[..size]);
        body = rest[size..].strip_prefix("\r\n").unwrap_or("");
    }
    out
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Decode an SSE body into the protocol events it carries.
///
/// Deliberately strict about the framing: one event per `data:` block, no
/// `event:` line, comments and the `retry:` directive ignored. A producer that
/// packed two events into one frame would pass a lenient parser and fail every
/// conforming client.
fn events(body: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for frame in body.split("\n\n") {
        let frame = frame.trim_end_matches('\n');
        if frame.is_empty() || frame.starts_with(':') || frame.starts_with("retry:") {
            continue;
        }
        let mut payload = String::new();
        for line in frame.lines() {
            let data = line
                .strip_prefix("data: ")
                .unwrap_or_else(|| panic!("an SSE frame carried something other than data: {line:?}"));
            if !payload.is_empty() {
                payload.push('\n');
            }
            payload.push_str(data);
        }
        out.push(serde_json::from_str(&payload).unwrap_or_else(|e| panic!("bad event JSON: {e} in {payload:?}")));
    }
    out
}

fn types(events: &[Value]) -> Vec<&str> {
    events.iter().map(|e| e["type"].as_str().unwrap()).collect()
}

/// Apply the run's snapshot and deltas the way a client would, so the final
/// state is the one the page would be showing.
fn replay_state(events: &[Value]) -> Value {
    let mut state = Value::Null;
    for e in events {
        match e["type"].as_str().unwrap() {
            "STATE_SNAPSHOT" => state = e["snapshot"].clone(),
            "STATE_DELTA" => {
                for op in e["delta"].as_array().expect("delta is an array") {
                    assert_eq!(op["op"], "replace", "the gauntlet only ever replaces");
                    let path = op["path"].as_str().unwrap();
                    let slot = state
                        .pointer_mut(path)
                        .unwrap_or_else(|| panic!("STATE_DELTA replaced {path}, which does not exist"));
                    *slot = op["value"].clone();
                }
            }
            _ => {}
        }
    }
    state
}

#[test]
fn the_stream_is_a_well_formed_ag_ui_run() {
    let server = Server::start();
    let (head, body) = server.get(&format!("/agui?pace=0&filter={FILTER}"));

    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(
        head.to_lowercase().contains("content-type: text/event-stream"),
        "the AG-UI HTTP binding answers with text/event-stream:\n{head}"
    );

    let events = events(&body);
    let types = types(&events);
    assert!(!events.is_empty(), "the run produced nothing");

    // Opening and closing, which the spec fixes: a stream MUST begin with
    // RUN_STARTED, and nothing may follow the terminal event.
    assert_eq!(types[0], "RUN_STARTED", "a stream must open with RUN_STARTED");
    assert_eq!(
        *types.last().unwrap(),
        "RUN_FINISHED",
        "a stream must end with its terminal event, not stop mid-run"
    );
    assert_eq!(
        types.iter().filter(|t| **t == "RUN_STARTED").count(),
        1,
        "a nested or repeated RUN_STARTED is a protocol violation"
    );
    assert_eq!(
        types.iter().filter(|t| matches!(**t, "RUN_FINISHED" | "RUN_ERROR")).count(),
        1,
        "exactly one terminal event"
    );
    assert_eq!(events[0]["threadId"], events.last().unwrap()["threadId"]);
    assert_eq!(events[0]["runId"], events.last().unwrap()["runId"]);

    // The state snapshot comes before anything tries to patch it.
    let first_snapshot = types.iter().position(|t| *t == "STATE_SNAPSHOT");
    let first_delta = types.iter().position(|t| *t == "STATE_DELTA");
    assert!(first_snapshot.is_some(), "the run must publish a state snapshot");
    assert!(
        first_snapshot < first_delta,
        "a delta before the snapshot leaves the client patching a state it has never seen"
    );
    assert_eq!(
        types.iter().filter(|t| **t == "STATE_SNAPSHOT").count(),
        1,
        "one snapshot at the start, then deltas — resending it per file would be megabytes of JPEG"
    );

    // Steps: opened and closed, matched by name, none left open at the end.
    let mut open: Vec<&str> = Vec::new();
    let mut closed = Vec::new();
    for e in &events {
        match e["type"].as_str().unwrap() {
            "STEP_STARTED" => {
                let name = e["stepName"].as_str().expect("stepName");
                assert!(!open.contains(&name), "{name} was opened twice");
                open.push(name);
            }
            "STEP_FINISHED" => {
                let name = e["stepName"].as_str().expect("stepName");
                let at = open.iter().position(|n| *n == name).unwrap_or_else(|| {
                    panic!("STEP_FINISHED for {name}, which was never opened")
                });
                open.remove(at);
                closed.push(name);
            }
            _ => {}
        }
    }
    assert!(open.is_empty(), "steps left open at RUN_FINISHED: {open:?}");
    assert_eq!(closed.len(), 4, "one step per file: {closed:?}");
    for name in ["ag-psd/layers.psd", "ag-psd/groups.psd", "ag-psd/effects.psd", "ag-psd/32bits.psd"] {
        assert!(closed.contains(&name), "{name} never ran");
    }

    // Text messages are balanced too, or the verifier would have refused the
    // terminal event.
    assert_eq!(
        types.iter().filter(|t| **t == "TEXT_MESSAGE_START").count(),
        types.iter().filter(|t| **t == "TEXT_MESSAGE_END").count()
    );
}

#[test]
fn the_state_has_the_shape_the_front_end_reads_and_the_tallies_add_up() {
    let server = Server::start();
    let (_, body) = server.get(&format!("/gauntlet/stream?pace=0&filter={FILTER}"));
    let events = events(&body);
    let state = replay_state(&events);

    // The documented shape, key by key.
    for key in ["total", "comparable", "index", "phase", "current", "tally", "histogram", "worst", "throughput"] {
        assert!(state.get(key).is_some(), "the state is missing {key}\n{state:#}");
    }
    assert_eq!(state["total"], 4, "the filter selected four files");
    assert_eq!(state["index"], 4, "every selected file was reported");
    assert_eq!(state["phase"], "done");
    assert_eq!(state["histogram"].as_array().unwrap().len(), 32);

    // The tally is disjoint bands that sum to the files processed. This is the
    // arithmetic that stops a demo quietly dropping a failure.
    let t = &state["tally"];
    let sum: u64 = ["exact", "within1", "within3", "within10", "over", "structureOnly", "failed"]
        .iter()
        .map(|k| t[*k].as_u64().unwrap_or_else(|| panic!("tally.{k} is missing")))
        .sum();
    assert_eq!(sum, 4, "the tally must account for every file: {t:#}");
    assert_eq!(t["failed"], 0, "none of these four should fail outright");

    // The histogram counts exactly the comparable files, no more and no fewer.
    let counted: u64 = state["histogram"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).sum();
    assert_eq!(counted, state["comparable"].as_u64().unwrap());
    assert_eq!(
        state["comparable"].as_u64().unwrap() + t["structureOnly"].as_u64().unwrap() + t["failed"].as_u64().unwrap(),
        4
    );

    // 32bits.psd carries no usable composite. It has to be visible as that,
    // not as a pass and not as an absence.
    assert_eq!(t["structureOnly"], 1, "32bits.psd has no reference composite");

    // `current` is the last file, with its three pictures.
    let cur = &state["current"];
    for key in ["file", "width", "height", "layers", "mae", "p95", "verdict", "ms", "ours", "adobe", "diff"] {
        assert!(cur.get(key).is_some(), "current.{key} is missing\n{cur:#}");
    }
    assert!(cur["width"].as_u64().unwrap() > 0);

    // Somewhere in the run, a file we get right and a file we get wrong, both
    // carrying all three JPEGs.
    let currents: Vec<&Value> = events
        .iter()
        .filter(|e| e["type"] == "STATE_DELTA")
        .flat_map(|e| e["delta"].as_array().unwrap())
        .filter(|op| op["path"] == "/current")
        .map(|op| &op["value"])
        .collect();
    assert_eq!(currents.len(), 4, "one result per file");
    let effects = currents
        .iter()
        .find(|c| c["file"] == "ag-psd/effects.psd")
        .expect("effects.psd reported");
    assert!(
        effects["mae"].as_f64().unwrap() > 1.0,
        "effects.psd is a known miss; the demo must not flatter it: {effects:#}"
    );
    for key in ["ours", "adobe", "diff"] {
        assert!(
            effects[key].as_str().unwrap().starts_with("data:image/jpeg;base64,"),
            "the failure has to be shown, not just scored: current.{key}"
        );
    }
    let thirtytwo = currents
        .iter()
        .find(|c| c["file"] == "ag-psd/32bits.psd")
        .expect("32bits.psd reported");
    assert_eq!(thirtytwo["verdict"], "structureOnly");
    assert!(thirtytwo["mae"].is_null(), "a file with no reference has no error to report");
    assert!(thirtytwo["adobe"].is_null(), "and nothing of Photoshop's to show");
    assert!(
        thirtytwo["ours"].as_str().is_some_and(|s| s.starts_with("data:image/jpeg;base64,")),
        "but our own render is still shown"
    );

    // Throughput is measured, not assumed.
    let tp = &state["throughput"];
    assert!(tp["elapsedMs"].as_u64().unwrap() > 0);
    assert!(tp["filesPerSec"].as_f64().unwrap() > 0.0);
    assert!(tp["megapixelsPerSec"].as_f64().unwrap() > 0.0);

    // The worst list is sorted and no longer than five.
    let worst = state["worst"].as_array().unwrap();
    assert!(worst.len() <= 5);
    let maes: Vec<f64> = worst.iter().map(|w| w["mae"].as_f64().unwrap()).collect();
    assert!(maes.windows(2).all(|w| w[0] >= w[1]), "the worst list must be sorted: {maes:?}");
    assert_eq!(worst.first().unwrap()["file"], "ag-psd/effects.psd");
}

#[test]
fn a_second_run_does_not_trample_the_first() {
    let server = Server::start();
    // Two runs over different subsets at the same time. Each must come back
    // with only its own files: one shared Progress would cross-contaminate the
    // tallies, and one shared cancellation would kill both.
    let (a, b) = std::thread::scope(|s| {
        let one = s.spawn(|| server.get("/agui?pace=0&filter=ag-psd/layers.psd,ag-psd/groups.psd"));
        let two = s.spawn(|| server.get("/agui?pace=0&filter=ag-psd/effects.psd"));
        (one.join().unwrap(), two.join().unwrap())
    });

    let first = replay_state(&events(&a.1));
    let second = replay_state(&events(&b.1));
    assert_eq!(first["total"], 2);
    assert_eq!(first["index"], 2);
    assert_eq!(second["total"], 1);
    assert_eq!(second["index"], 1);
    assert_eq!(second["current"]["file"], "ag-psd/effects.psd");
    assert_ne!(
        events(&a.1)[0]["runId"],
        events(&b.1)[0]["runId"],
        "each connection is its own run"
    );
}

#[test]
fn the_filter_and_the_pace_are_honoured() {
    let server = Server::start();

    let (_, one) = server.get("/agui?pace=0&filter=ag-psd/groups.psd");
    let state = replay_state(&events(&one));
    assert_eq!(state["total"], 1, "?filter= runs a subset, not the whole corpus");

    // A pace makes the same run take longer. 300 ms over two files is
    // measurable without making the test slow.
    let started = Instant::now();
    let (_, paced) = server.get("/agui?pace=300&filter=ag-psd/layers.psd,ag-psd/groups.psd");
    let took = started.elapsed();
    assert_eq!(replay_state(&events(&paced))["index"], 2);
    assert!(
        took >= Duration::from_millis(500),
        "?pace=300 over two files should take at least 600 ms, took {took:?}"
    );

    let (_, none) = server.get("/agui?pace=0&filter=nothing-matches-this");
    let state = replay_state(&events(&none));
    assert_eq!(state["total"], 0, "a filter that matches nothing is an empty run, not an error");
    assert_eq!(types(&events(&none))[0], "RUN_STARTED");
    assert_eq!(*types(&events(&none)).last().unwrap(), "RUN_FINISHED");
}

#[test]
fn the_page_and_the_health_endpoint_are_served_and_the_directory_is_not() {
    let server = Server::start();

    let (head, body) = server.get("/health");
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let health: Value = serde_json::from_str(body.trim()).expect("health is JSON");
    assert_eq!(health["ok"], true);
    assert!(health["files"].as_u64().unwrap() > 200, "the corpus is there: {health}");

    let (head, _) = server.get("/");
    assert!(head.starts_with("HTTP/1.1 200"), "the page is served at /: {head}");
    assert!(head.to_lowercase().contains("text/html"), "{head}");

    // The static server must not be a way out of its directory.
    let (head, _) = server.get("/../../Cargo.toml");
    assert!(
        head.starts_with("HTTP/1.1 404") || head.starts_with("HTTP/1.1 400"),
        "a traversal must be refused, not served: {head}"
    );
}
