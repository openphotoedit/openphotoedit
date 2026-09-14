//! `openphotoshop`: the editor as one local program.
//!
//! ```text
//! openphotoshop                          same as `serve`
//! openphotoshop serve [--port N] [--no-open] [--dir PATH] [--models-dir PATH]
//! openphotoshop open <file> [same flags]  serve, and hand <file> to the UI once
//! openphotoshop models path|list|fetch <id>… [--all] [--manifest PATH]
//! openphotoshop render <in> --op '<json>' [--op …] --out <out.png>
//! ```

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use editor_server::models;
use editor_server::server::{self, AppState};
use editor_server::web::WebSource;

#[derive(Parser)]
#[command(name = "openphotoshop", version, about = "OpenPhotoshop, served from this computer", propagate_version = true)]
struct Cli {
    /// More log output on stderr (-v requests, -vv debug).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the editor on 127.0.0.1 and open it in the browser.
    Serve(ServeArgs),
    /// Serve the editor and open <file> in it.
    Open {
        file: PathBuf,
        #[command(flatten)]
        serve: ServeArgs,
    },
    /// Manage AI model files.
    Models {
        #[command(subcommand)]
        command: ModelsCommand,
        /// Use this manifest instead of looking for models.json.
        #[arg(long, global = true)]
        manifest: Option<PathBuf>,
        /// Models folder (default: the per-user data folder).
        #[arg(long, global = true)]
        models_dir: Option<PathBuf>,
    },
    /// Run engine commands on an image natively and write a PNG (developer tool).
    Render {
        input: PathBuf,
        /// A command as JSON, or a JSON array of commands. Repeatable; run in order.
        #[arg(long = "op")]
        ops: Vec<String>,
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Args, Default, Clone)]
struct ServeArgs {
    /// Port on 127.0.0.1. Given: that port or fail. Omitted: 8765, or the next free one.
    #[arg(long)]
    port: Option<u16>,
    /// Do not open a browser; just print the address.
    #[arg(long)]
    no_open: bool,
    /// Serve the web app from this build folder instead of the copy inside the binary.
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Models folder (default: the per-user data folder).
    #[arg(long)]
    models_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum ModelsCommand {
    /// Print the models folder.
    Path,
    /// List the models in the manifest and whether each is installed.
    List,
    /// Download models, verifying sha256.
    Fetch {
        ids: Vec<String>,
        /// Every model in the manifest.
        #[arg(long)]
        all: bool,
    },
}

fn main() -> ExitCode {
    // Finder passed `-psn_0_12345` to apps on older macOS; it is not ours.
    let args = std::env::args_os().filter(|a| !a.to_string_lossy().starts_with("-psn_"));
    let cli = Cli::parse_from(args);
    init_logging(cli.verbose);

    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("could not start: {e}");
            return ExitCode::FAILURE;
        }
    };
    let launched_bare = cli.command.is_none();
    let result = rt.block_on(run(cli));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let msg = format!("{e:#}");
            eprintln!("openphotoshop: {msg}");
            if launched_bare {
                // Double-clicked: there is no terminal to read the error in.
                report_visibly(&msg);
            }
            ExitCode::FAILURE
        }
    }
}

fn init_logging(verbose: u8) {
    let level = match verbose {
        0 => tracing::Level::WARN,
        1 => tracing::Level::INFO,
        _ => tracing::Level::DEBUG,
    };
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(level)
        .with_target(false)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .compact()
        .init();
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command.unwrap_or(Command::Serve(ServeArgs::default())) {
        Command::Serve(args) => serve(args, None).await,
        Command::Open { file, serve: args } => {
            let meta = std::fs::metadata(&file).with_context(|| format!("could not open {}", file.display()))?;
            if !meta.is_file() {
                bail!("{} is not a file", file.display());
            }
            let path = file.canonicalize().with_context(|| format!("could not resolve {}", file.display()))?;
            serve(args, Some(path)).await
        }
        Command::Models { command, manifest, models_dir } => models_cmd(command, manifest, models_dir).await,
        Command::Render { input, ops, out } => {
            let ops = editor_server::render::parse_ops(&ops)?;
            let r = tokio::task::spawn_blocking(move || editor_server::render::render(&input, &ops, &out)).await??;
            println!(
                "{}",
                serde_json::json!({ "out": r.out.display().to_string(), "width": r.width, "height": r.height, "commands": r.commands, "layers": r.layers })
            );
            Ok(())
        }
    }
}

fn web_source(dir: Option<PathBuf>) -> Result<Option<WebSource>> {
    match dir {
        Some(d) => {
            if !d.join("index.html").is_file() {
                bail!("{} has no index.html. Build the web app first (npm run build in apps/web).", d.display());
            }
            Ok(Some(WebSource::Dir(d.canonicalize()?)))
        }
        None => {
            let src = WebSource::default_for_build();
            if src.is_none() {
                bail!("this build has no web app inside; pass --dir apps/web/dist");
            }
            Ok(src)
        }
    }
}

async fn serve(args: ServeArgs, open_file: Option<PathBuf>) -> Result<()> {
    let web = web_source(args.dir)?;
    if let Some(WebSource::Dir(d)) = &web {
        tracing::info!(dir = %d.display(), "serving the web app from disk");
    }
    let models_dir = args.models_dir.unwrap_or_else(models::default_dir);
    let state = Arc::new(AppState::new(web, models_dir.clone()));
    let listener = server::bind(args.port).await?;
    let addr = listener.local_addr()?;

    let mut url = format!("http://{addr}/");
    if let Some(path) = open_file {
        let token = state.tokens.issue(path);
        url.push_str(&format!("?open={token}"));
    }
    println!("OpenPhotoshop is running at http://{addr}/");
    println!("Models folder: {}", models_dir.display());
    println!("Press Ctrl-C to stop.");
    if args.no_open {
        if url.contains('?') {
            println!("Open: {url}");
        }
    } else {
        open_browser(&url);
    }

    let app = server::router(state);
    axum::serve(listener, app).with_graceful_shutdown(server::shutdown_signal()).await.context("the server stopped")?;
    eprintln!("Stopped.");
    Ok(())
}

async fn models_cmd(command: ModelsCommand, manifest: Option<PathBuf>, models_dir: Option<PathBuf>) -> Result<()> {
    let dir = models_dir.unwrap_or_else(models::default_dir);
    if let ModelsCommand::Path = command {
        println!("{}", dir.display());
        return Ok(());
    }
    let web = WebSource::default_for_build();
    let (m, source) = models::load_manifest(manifest.as_deref(), &dir, web.as_ref()).await?;
    match command {
        ModelsCommand::Path => unreachable!(),
        ModelsCommand::List => {
            eprintln!("manifest: {source}");
            eprintln!("folder:   {}", dir.display());
            if m.models.is_empty() {
                println!("The manifest lists no models.");
            }
            for e in &m.models {
                let size = e.size.map(human).unwrap_or_else(|| "?".into());
                let state = if models::installed(e, &dir) { "installed" } else { "-" };
                println!("{:<28} {:>9}  {:<10} {}", e.id, size, state, e.license.as_deref().unwrap_or(""));
            }
            Ok(())
        }
        ModelsCommand::Fetch { ids, all } => {
            let wanted: Vec<&models::ModelEntry> = if all {
                m.models.iter().collect()
            } else {
                if ids.is_empty() {
                    bail!("name a model to fetch, or pass --all. `openphotoshop models list` shows them.");
                }
                ids.iter().map(|id| m.find(id).with_context(|| format!("no model {id:?} in {source}"))).collect::<Result<_>>()?
            };
            let client = reqwest::Client::builder().user_agent(concat!("openphotoshop/", env!("CARGO_PKG_VERSION"))).build()?;
            let progress = std::io::IsTerminal::is_terminal(&std::io::stderr());
            let mut failed = 0;
            for e in wanted {
                match models::fetch(&client, e, &dir, progress).await {
                    Ok(models::Fetched::Downloaded) => println!("{}: downloaded and verified", e.id),
                    Ok(models::Fetched::AlreadyPresent) => println!("{}: already installed", e.id),
                    Err(err) => {
                        failed += 1;
                        eprintln!("{}: {err:#}", e.id);
                    }
                }
            }
            if failed > 0 {
                bail!("{failed} model(s) could not be fetched");
            }
            Ok(())
        }
    }
}

fn human(n: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if n as f64 >= MB {
        format!("{:.0} MB", n as f64 / MB)
    } else {
        format!("{:.0} KB", n as f64 / 1024.0)
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let opened = std::process::Command::new("open").arg(url).spawn().is_ok();
    #[cfg(target_os = "windows")]
    let opened = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn().is_ok();
    #[cfg(all(unix, not(target_os = "macos")))]
    let opened = std::process::Command::new("xdg-open").arg(url).spawn().is_ok();
    if !opened {
        println!("Open this in your browser: {url}");
    }
}

fn report_visibly(message: &str) {
    #[cfg(target_os = "macos")]
    {
        let script = format!("display dialog {message:?} with title \"OpenPhotoshop\" buttons {{\"OK\"}} default button 1 with icon caution");
        let _ = std::process::Command::new("osascript").args(["-e", &script]).status();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = message;
}
