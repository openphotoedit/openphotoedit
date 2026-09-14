//! Decide which folder gets embedded as the web app.
//!
//! `apps/web/dist` is a build output that is not in git, so a fresh checkout
//! (or CI running `cargo clippy` before `npm run build`) has no such folder.
//! rust-embed refuses to compile a missing folder, and a server crate that
//! cannot even be type-checked without Node is a trap. So when the build is
//! missing we embed a one-page placeholder that says how to fix it, and the
//! binary reports `embedded: "placeholder"` in `/api/health`.
//!
//! `OPENPHOTOSHOP_WEB_DIST` points at a different build if you need one.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=OPENPHOTOSHOP_WEB_DIST");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let dist = match std::env::var("OPENPHOTOSHOP_WEB_DIST") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => manifest.join("../../apps/web/dist"),
    };
    let index = dist.join("index.html");
    // Watching index.html is enough to notice a rebuild (Vite rewrites it
    // with new hashed asset names). rust-embed tracks the files themselves.
    println!("cargo:rerun-if-changed={}", index.display());
    println!("cargo:rerun-if-changed={}", dist.display());

    let (folder, placeholder) = if index.is_file() {
        (dist.canonicalize().unwrap_or(dist), false)
    } else {
        let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("placeholder-web");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("index.html"), PLACEHOLDER).unwrap();
        println!(
            "cargo:warning=apps/web/dist is missing; embedding a placeholder page. Run `npm run build` in apps/web, then rebuild."
        );
        (out, true)
    };
    println!("cargo:rustc-env=OPENPHOTOSHOP_EMBED_DIR={}", folder.display());
    println!("cargo:rustc-env=OPENPHOTOSHOP_EMBED_PLACEHOLDER={}", if placeholder { "1" } else { "0" });
}

const PLACEHOLDER: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>OpenPhotoshop</title>
<style>
  body { font: 15px/1.5 system-ui, sans-serif; max-width: 40rem; margin: 4rem auto; padding: 0 1rem; color: #1a1a1a; background: #fafafa; }
  code { background: #eee; padding: 0.1rem 0.3rem; border-radius: 4px; }
  @media (prefers-color-scheme: dark) { body { color: #eee; background: #111; } code { background: #222; } }
</style>
</head>
<body>
<h1>This build has no web app inside</h1>
<p>The server is running, but it was compiled before the web app was built, so there is nothing to show.</p>
<p>Build the web app with <code>npm run build</code> in <code>apps/web</code>, then rebuild the binary. Or run <code>openphotoshop serve --dir apps/web/dist</code> to serve a build from disk.</p>
</body>
</html>
"#;
