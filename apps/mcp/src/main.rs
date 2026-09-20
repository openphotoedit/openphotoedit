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

mod catalog;
mod engine;
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

    let sandbox = sandbox::Sandbox::new(parse_roots(&args)?)?;
    tracing::info!(
        roots = ?sandbox.root_names(),
        tools = 22,
        "openphotoedit-mcp ready"
    );

    let service = server::OpenPhotoEdit::new(sandbox)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

fn parse_roots(args: &[String]) -> anyhow::Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                let dir = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow::anyhow!("--root needs a directory"))?;
                roots.push(PathBuf::from(dir));
                i += 2;
            }
            other => anyhow::bail!("unexpected argument {other:?} — run with --help"),
        }
    }
    Ok(roots)
}

fn print_usage() {
    println!(
        "openphotoedit-mcp — local photo editing for AI agents (MCP, stdio)\n\n\
         USAGE:\n\
         \x20   openphotoedit-mcp [--root DIR]...\n\n\
         \x20   --root DIR   A directory the server may read and write.\n\
         \x20                Repeatable. Defaults to the working directory.\n\
         \x20   --version    Print the version.\n\n\
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
    use super::parse_roots;

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
}
