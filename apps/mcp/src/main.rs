//! `openphotoedit-mcp` — OpenPhotoEdit's editing engine, over the Model
//! Context Protocol.
//!
//! One binary, stdio transport, and every operation runs here on this
//! machine. The photo-editing tools an agent can reach today are nearly all
//! wrappers over a cloud API, so using one means uploading the photograph.
//! This one opens the file, edits it and writes it back, and hands the agent
//! a sentence about what happened. The pixels never leave.
//!
//! ```text
//! openphotoedit-mcp --root ~/Pictures --root ~/Downloads
//! ```
//!
//! With no `--root`, the working directory is the only reachable place.
//!
//! The design follows our sibling server, `openpdfedit-mcp`: paths in and
//! paths out, a sandbox the operator sets and a tool call cannot widen, and
//! stdout reserved for the protocol.

// `as_chunks` postdates the workspace's minimum Rust (1.85), as in the
// engine crates.
#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]

mod agui;
mod catalog;
mod engine;
mod gauntlet;
mod sandbox;
mod server;
mod sessions;
mod tools;

use std::path::PathBuf;

use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // stderr, never stdout. stdout *is* the protocol channel here: one stray
    // log line on it is a parse error at the other end, not a cosmetic
    // problem, and the symptom is a client that mysteriously will not
    // connect.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("openphotoedit-mcp {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let parsed = parse_args(&args)?;
    let sandbox = sandbox::Sandbox::new(parsed.roots.clone())?;
    tracing::info!(
        roots = ?sandbox.root_names(),
        tools = if parsed.agui_port.is_some() { 23 } else { 22 },
        "openphotoedit-mcp ready"
    );

    // The Photoshop gauntlet, if it was asked for. Bound here rather than
    // inside the spawned task so a port already in use stops the process with
    // a message instead of leaving a server that is silently not listening.
    if let Some(port) = parsed.agui_port {
        let serving = agui::bind(agui::Config {
            port,
            corpus: parsed
                .corpus
                .unwrap_or_else(gauntlet::corpus::default_root),
            ui_dir: parsed.ui_dir.unwrap_or_else(default_ui_dir),
            pace_ms: parsed.pace_ms.unwrap_or(gauntlet::DEFAULT_PACE_MS),
        })
        .await?;
        tokio::spawn(async move {
            if let Err(e) = serving.run().await {
                tracing::error!(error = %e, "the gauntlet server stopped");
            }
        });
    }

    if parsed.no_stdio {
        // A demo run from a terminal: there is no MCP client on stdin, and
        // starting the stdio service would read EOF and exit immediately.
        if parsed.agui_port.is_none() {
            anyhow::bail!("--no-stdio only makes sense with --agui-port");
        }
        tokio::signal::ctrl_c().await?;
        return Ok(());
    }

    let service = server::OpenPhotoEdit::new(sandbox)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

/// The front end the gauntlet serves. `apps/mcp/ui/` belongs to the viewer
/// workstream; this is a path, read at run time, never compiled in — so the
/// page can be edited while the server is up.
fn default_ui_dir() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/gauntlet");
    here.canonicalize().unwrap_or(here)
}

/// Everything the command line can say.
#[derive(Debug, Default)]
struct Args {
    roots: Vec<PathBuf>,
    /// Serve the Photoshop gauntlet on this port, loopback only. 0 lets the
    /// OS choose, which the integration test uses.
    agui_port: Option<u16>,
    corpus: Option<PathBuf>,
    ui_dir: Option<PathBuf>,
    pace_ms: Option<u64>,
    no_stdio: bool,
}

fn parse_args(args: &[String]) -> anyhow::Result<Args> {
    let mut out = Args::default();
    let mut i = 0;
    let value = |i: usize, flag: &str| -> anyhow::Result<&String> {
        args.get(i + 1)
            .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
    };
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                out.roots.push(PathBuf::from(value(i, "--root")?));
                i += 2;
            }
            "--agui-port" => {
                let v = value(i, "--agui-port")?;
                out.agui_port = Some(
                    v.parse()
                        .map_err(|_| anyhow::anyhow!("--agui-port wants a port number, not {v:?}"))?,
                );
                i += 2;
            }
            "--gauntlet-corpus" => {
                out.corpus = Some(PathBuf::from(value(i, "--gauntlet-corpus")?));
                i += 2;
            }
            "--gauntlet-ui" => {
                out.ui_dir = Some(PathBuf::from(value(i, "--gauntlet-ui")?));
                i += 2;
            }
            "--pace" => {
                let v = value(i, "--pace")?;
                out.pace_ms = Some(
                    v.parse()
                        .map_err(|_| anyhow::anyhow!("--pace wants milliseconds, not {v:?}"))?,
                );
                i += 2;
            }
            "--no-stdio" => {
                out.no_stdio = true;
                i += 1;
            }
            other => anyhow::bail!("unexpected argument {other:?} — run with --help"),
        }
    }
    Ok(out)
}

/// The roots alone, which is all the older tests care about.
#[cfg(test)]
fn parse_roots(args: &[String]) -> anyhow::Result<Vec<PathBuf>> {
    Ok(parse_args(args)?.roots)
}

fn print_usage() {
    println!(
        "openphotoedit-mcp — local photo editing for AI agents (MCP, stdio)\n\n\
         USAGE:\n\
         \x20   openphotoedit-mcp [--root DIR]... [--agui-port N]\n\n\
         \x20   --root DIR   A directory the server may read and write.\n\
         \x20                Repeatable. Defaults to the working directory.\n\
         \x20   --version    Print the version.\n\n\
         THE PHOTOSHOP GAUNTLET (a demo, served over AG-UI):\n\
         \x20   --agui-port N        Serve it on 127.0.0.1:N. Loopback only.\n\
         \x20   --gauntlet-corpus D  The .psd/.psb corpus. Default: testdata/psd.\n\
         \x20   --gauntlet-ui D      The page to serve at /. Default: apps/mcp/ui/gauntlet.\n\
         \x20   --pace MS            Pause after each file so a person can watch\n\
         \x20                        (default 120; a request may override with ?pace=).\n\
         \x20   --no-stdio           Serve only the demo; do not speak MCP on stdin.\n\n\
         TOOLS:\n\
         \x20   list_operations     the engine's whole command catalogue\n\
         \x20   run_operations      run any list of those commands on a file\n\
         \x20   image_info          size, format, layers, EXIF, histogram\n\
         \x20   convert_image       write a copy in another format\n\
         \x20   resize_image        resample to a new size\n\
         \x20   crop_image          crop to a rectangle\n\
         \x20   rotate_flip_image   rotate and mirror\n\
         \x20   adjust_image        exposure, contrast, colour, levels, curves\n\
         \x20   filter_image        one filter from the filter domain\n\
         \x20   psd_info            the layer tree of a PSD/PSB/.opproj\n\
         \x20   psd_flatten         composite a layered file to one image\n\
         \x20   psd_export_layers   each layer to its own PNG\n\
         \x20   raw_develop         develop a camera RAW file\n\
         \x20   batch_edit          one operation list over a glob\n\
         \x20   open_document       open a file and keep it open\n\
         \x20   apply_operations    edit an open document\n\
         \x20   undo / redo         step through its history\n\
         \x20   document_state      what is open and what it looks like\n\
         \x20   save_document       write an open document out\n\
         \x20   close_document      close it\n\
         \x20   render_preview      a small JPEG, for the viewer\n\n\
         Every operation runs on this machine. Photographs are never uploaded,\n\
         and no tool returns image data unless you ask for a preview.\n"
    );
}

#[cfg(test)]
mod tests {
    use super::{parse_args, parse_roots};

    #[test]
    fn roots_accumulate() {
        let args = ["--root", "/a", "--root", "/b"].map(String::from).to_vec();
        assert_eq!(parse_roots(&args).unwrap().len(), 2);
    }

    #[test]
    fn no_roots_means_the_working_directory() {
        assert!(parse_roots(&[]).unwrap().is_empty());
    }

    #[test]
    fn a_dangling_root_flag_is_an_error() {
        assert!(parse_roots(&["--root".to_string()]).is_err());
    }

    #[test]
    fn an_unknown_flag_is_refused_rather_than_ignored() {
        // Quietly ignoring a misspelled --roots would start a server
        // confined somewhere the user did not mean.
        assert!(parse_roots(&["--roots".to_string(), "/a".to_string()]).is_err());
    }

    fn argv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_gauntlet_flags_parse_alongside_the_roots() {
        let a = parse_args(&argv(&[
            "--root", "/a",
            "--agui-port", "5261",
            "--gauntlet-corpus", "/c",
            "--gauntlet-ui", "/u",
            "--pace", "0",
            "--no-stdio",
        ]))
        .unwrap();
        assert_eq!(a.roots.len(), 1);
        assert_eq!(a.agui_port, Some(5261));
        assert_eq!(a.corpus.unwrap().to_str(), Some("/c"));
        assert_eq!(a.ui_dir.unwrap().to_str(), Some("/u"));
        assert_eq!(a.pace_ms, Some(0));
        assert!(a.no_stdio);
    }

    #[test]
    fn no_gauntlet_flags_means_no_http_server_at_all() {
        let a = parse_args(&argv(&["--root", "/a"])).unwrap();
        assert_eq!(a.agui_port, None);
        assert!(!a.no_stdio);
    }

    #[test]
    fn a_port_that_is_not_a_number_is_refused_at_startup() {
        assert!(parse_args(&argv(&["--agui-port", "five thousand"])).is_err());
        assert!(parse_args(&argv(&["--agui-port"])).is_err());
        assert!(parse_args(&argv(&["--pace", "slow"])).is_err());
    }
}
