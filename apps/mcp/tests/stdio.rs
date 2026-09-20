//! Speak MCP to the real binary, over real stdio, against real photographs.
//!
//! The unit tests check the pieces. This checks the thing people actually
//! install: a process, a pipe, JSON-RPC, and files on disk afterwards that
//! are opened again and measured. Everything it asserts is something a
//! client would notice — the handshake, the tool list, the UI resource, the
//! written pixels, the refusals.
//!
//! The rule it guards most carefully is the one that is easiest to break by
//! accident: **no tool returns image data unless `include_preview` was
//! set**. [`Client::call_tool`] checks that on every single call, so a tool
//! that starts leaking pixels fails this file wherever it is called from.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

const BINARY: &str = env!("CARGO_BIN_EXE_openphotoedit-mcp");

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("apps/mcp lives in the repository")
}

fn testdata() -> PathBuf {
    repo_root().join("testdata")
}

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    /// Start the server with the repository's testdata readable and a
    /// scratch directory writable.
    fn start(out_dir: &Path) -> Client {
        let mut child = Command::new(BINARY)
            .arg("--root")
            .arg(testdata())
            .arg("--root")
            .arg(out_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Logs go to stderr and stay there. If they ever went to stdout
            // the very next read would fail to parse, which is the point.
            .stderr(Stdio::null())
            .spawn()
            .expect("the server binary should start");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut client = Client {
            child,
            stdin,
            stdout,
            next_id: 0,
        };
        let init = client.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "stdio-test", "version": "0" }
            }),
        );
        assert_eq!(init["serverInfo"]["name"], "openphotoedit");
        let instructions = init["instructions"].as_str().unwrap_or_default();
        assert!(
            instructions.contains("never uploaded"),
            "the handshake should say where the files go: {instructions}"
        );
        assert!(
            instructions.contains("no AI tools") || instructions.contains("no AI"),
            "the handshake should say what is absent"
        );
        client.notify("notifications/initialized", json!({}));
        client
    }

    fn send(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").expect("write to the server");
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// Send a request and read until its answer comes back.
    fn raw_request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line).expect("read from server");
            assert!(read > 0, "the server closed the connection during {method}");
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                panic!("stdout must carry only JSON-RPC, got: {line}");
            };
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return value;
            }
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let value = self.raw_request(method, params);
        assert!(
            value.get("error").is_none(),
            "{method} failed: {}",
            value["error"]
        );
        value["result"].clone()
    }

    /// Call a tool and insist on the no-pixels rule.
    fn call_tool(&mut self, name: &str, args: Value) -> Value {
        let wants_image = args
            .get("include_preview")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || name == "render_preview";
        let result = self.request(
            "tools/call",
            json!({ "name": name, "arguments": args.clone() }),
        );
        assert_ne!(
            result["isError"],
            json!(true),
            "{name} returned an error: {}",
            result["content"]
        );
        let images = image_blocks(&result);
        if wants_image {
            assert_eq!(images, 1, "{name} should have returned one preview");
        } else {
            assert_eq!(
                images, 0,
                "{name} returned image data without include_preview — that is the one thing \
                 this server must never do. Arguments: {args}"
            );
            let text = result["content"].to_string();
            assert!(
                !text.contains("data:image"),
                "{name} smuggled a data URI into its text content"
            );
            assert!(
                result["structuredContent"]
                    .get("preview_data_uri")
                    .is_none(),
                "{name} put a preview data URI in structuredContent without being asked"
            );
        }
        result
    }

    /// A tool call that is expected to fail.
    fn call_tool_expecting_failure(&mut self, name: &str, args: Value) -> String {
        let value = self.raw_request(
            "tools/call",
            json!({ "name": name, "arguments": args }),
        );
        if let Some(error) = value.get("error") {
            return error["message"].as_str().unwrap_or_default().to_string();
        }
        let result = &value["result"];
        assert_eq!(
            result["isError"],
            json!(true),
            "{name} was expected to fail, and did not: {result}"
        );
        result["content"].to_string()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn image_blocks(result: &Value) -> usize {
    result["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b["type"] == "image")
                .count()
        })
        .unwrap_or(0)
}

fn text_of(result: &Value) -> String {
    result["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn dimensions(path: &Path) -> (u32, u32) {
    let img = image::open(path).unwrap_or_else(|e| panic!("{} did not open: {e}", path.display()));
    (img.width(), img.height())
}

fn photo(name: &str) -> String {
    testdata()
        .join("photos")
        .join(name)
        .display()
        .to_string()
}

/// One session, one server, the whole surface.
///
/// Deliberately a single test: starting the binary and decoding a 12 MB RAW
/// file are the slow parts, and doing them once is worth more than the
/// isolation of separate tests would buy.
#[test]
fn the_server_does_what_it_says_over_real_stdio() {
    let out = tempfile::tempdir().unwrap();
    let out_dir = out.path().canonicalize().unwrap();
    let mut mcp = Client::start(&out_dir);

    // --- the tool list ---------------------------------------------------
    let listed = mcp.request("tools/list", json!({}));
    let tools = listed["tools"].as_array().expect("a list of tools");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    for expected in [
        "list_operations",
        "run_operations",
        "image_info",
        "convert_image",
        "resize_image",
        "crop_image",
        "rotate_flip_image",
        "adjust_image",
        "filter_image",
        "psd_info",
        "psd_flatten",
        "psd_export_layers",
        "raw_develop",
        "batch_edit",
        "open_document",
        "apply_operations",
        "undo",
        "redo",
        "document_state",
        "save_document",
        "close_document",
        "render_preview",
    ] {
        assert!(names.contains(&expected), "{expected} is missing from tools/list");
    }
    assert_eq!(names.len(), 22, "the tool list changed: {names:?}");
    for tool in tools {
        assert!(
            tool["description"].as_str().unwrap_or("").len() > 40,
            "{} has no useful description",
            tool["name"]
        );
        assert!(tool["inputSchema"].is_object(), "{} has no schema", tool["name"]);
    }

    // --- the UI wiring ---------------------------------------------------
    let viewer_tool = tools
        .iter()
        .find(|t| t["name"] == "image_info")
        .expect("image_info");
    assert_eq!(
        viewer_tool["_meta"]["ui"]["resourceUri"],
        "ui://openphotoedit/viewer"
    );
    assert_eq!(
        viewer_tool["_meta"]["ui"]["visibility"],
        json!(["model", "app"])
    );
    let app_tool = tools
        .iter()
        .find(|t| t["name"] == "render_preview")
        .expect("render_preview");
    assert_eq!(app_tool["_meta"]["ui"]["visibility"], json!(["app"]));

    let resources = mcp.request("resources/list", json!({}));
    let resource = &resources["resources"][0];
    assert_eq!(resource["uri"], "ui://openphotoedit/viewer");
    assert_eq!(resource["mimeType"], "text/html;profile=mcp-app");

    let read = mcp.request(
        "resources/read",
        json!({ "uri": "ui://openphotoedit/viewer" }),
    );
    let contents = &read["contents"][0];
    assert_eq!(contents["mimeType"], "text/html;profile=mcp-app");
    assert!(
        contents["text"].as_str().unwrap().contains("<html"),
        "the viewer resource should be an HTML page"
    );
    let missing = mcp.raw_request("resources/read", json!({ "uri": "ui://nope" }));
    assert!(missing.get("error").is_some(), "an unknown URI should fail");

    // --- the catalogue ---------------------------------------------------
    let catalogue = mcp.call_tool("list_operations", json!({}));
    let count = catalogue["structuredContent"]["count"].as_u64().unwrap();
    assert!(count > 130, "only {count} operations listed");
    let filters = mcp.call_tool("list_operations", json!({ "domain": "filter" }));
    let filter_count = filters["structuredContent"]["count"].as_u64().unwrap();
    assert!(filter_count > 30, "only {filter_count} filters listed");
    assert!(text_of(&filters).contains("filter.gaussian-blur"));
    let bad_domain = mcp.call_tool_expecting_failure(
        "list_operations",
        json!({ "domain": "sorcery" }),
    );
    assert!(bad_domain.contains("filter"), "it should list the real domains");

    // --- image_info on a real photograph ---------------------------------
    let info = mcp.call_tool("image_info", json!({ "path": photo("landscape.jpg") }));
    let data = &info["structuredContent"];
    let (jw, jh) = (
        data["width"].as_u64().unwrap(),
        data["height"].as_u64().unwrap(),
    );
    assert!(jw > 100 && jh > 100, "{jw}×{jh} is not a photograph");
    assert_eq!(data["format"], "jpeg");
    assert_eq!(data["has_alpha"], false);
    assert!(
        data["histogram"]["l"].as_array().unwrap().len() == 256,
        "a histogram has 256 buckets"
    );
    assert!(data["exif"].is_object(), "a camera JPEG should carry EXIF");

    // --- resize, and check the pixels that came out ----------------------
    let resized = out_dir.join("landscape-800.png");
    let result = mcp.call_tool(
        "resize_image",
        json!({
            "input_path": photo("landscape.jpg"),
            "output_path": resized.display().to_string(),
            "width": 800
        }),
    );
    assert!(resized.exists(), "resize_image wrote nothing");
    let (rw, rh) = dimensions(&resized);
    assert_eq!(rw, 800);
    let expected_h = (800.0 * jh as f64 / jw as f64).round() as u32;
    assert!(
        rh.abs_diff(expected_h) <= 1,
        "{rh} does not keep the aspect ratio (expected about {expected_h})"
    );
    assert_eq!(result["structuredContent"]["width"], 800);

    // Writing it again is refused, and the file is left alone.
    let before = std::fs::metadata(&resized).unwrap().len();
    let refusal = mcp.call_tool_expecting_failure(
        "resize_image",
        json!({
            "input_path": photo("landscape.jpg"),
            "output_path": resized.display().to_string(),
            "width": 400
        }),
    );
    assert!(refusal.contains("overwrite"), "{refusal}");
    assert_eq!(std::fs::metadata(&resized).unwrap().len(), before);

    // With overwrite, it goes through.
    mcp.call_tool(
        "resize_image",
        json!({
            "input_path": photo("landscape.jpg"),
            "output_path": resized.display().to_string(),
            "width": 400,
            "overwrite": true
        }),
    );
    assert_eq!(dimensions(&resized).0, 400);

    // --- the sandbox holds ------------------------------------------------
    let escape = mcp.call_tool_expecting_failure(
        "image_info",
        json!({ "path": format!("{}/../../../../etc/hosts", testdata().display()) }),
    );
    assert!(
        escape.contains("outside") || escape.contains("nothing readable"),
        "a traversal out of the roots must be refused: {escape}"
    );
    let escape_write = mcp.call_tool_expecting_failure(
        "convert_image",
        json!({
            "input_path": photo("product.jpg"),
            "output_path": "/tmp/openphotoedit-mcp-should-not-exist.png"
        }),
    );
    assert!(escape_write.contains("outside"), "{escape_write}");
    assert!(!Path::new("/tmp/openphotoedit-mcp-should-not-exist.png").exists());

    // --- crop, with the rectangle checked --------------------------------
    let cropped = out_dir.join("crop.png");
    mcp.call_tool(
        "crop_image",
        json!({
            "input_path": photo("portrait.jpg"),
            "output_path": cropped.display().to_string(),
            "x": 10, "y": 20, "width": 120, "height": 90
        }),
    );
    assert_eq!(dimensions(&cropped), (120, 90));
    let off_the_edge = mcp.call_tool_expecting_failure(
        "crop_image",
        json!({
            "input_path": photo("portrait.jpg"),
            "output_path": out_dir.join("nope.png").display().to_string(),
            "x": 0, "y": 0, "width": 99_999, "height": 10
        }),
    );
    assert!(off_the_edge.contains("outside"), "{off_the_edge}");

    // --- rotate ----------------------------------------------------------
    let rotated = out_dir.join("rotated.png");
    mcp.call_tool(
        "rotate_flip_image",
        json!({
            "input_path": cropped.display().to_string(),
            "output_path": rotated.display().to_string(),
            "rotate": 90
        }),
    );
    assert_eq!(dimensions(&rotated), (90, 120), "a quarter turn swaps the sides");

    // --- adjust, and check it actually brightened ------------------------
    let brighter = out_dir.join("brighter.png");
    mcp.call_tool(
        "adjust_image",
        json!({
            "input_path": cropped.display().to_string(),
            "output_path": brighter.display().to_string(),
            "exposure": 1.0
        }),
    );
    let before_px = image::open(&cropped).unwrap().into_rgb8();
    let after_px = image::open(&brighter).unwrap().into_rgb8();
    let mean = |img: &image::RgbImage| {
        img.pixels().map(|p| p[0] as u64).sum::<u64>() as f64 / img.pixels().len() as f64
    };
    assert!(
        mean(&after_px) > mean(&before_px) + 5.0,
        "exposure +1 should brighten: {} → {}",
        mean(&before_px),
        mean(&after_px)
    );

    // --- filter ----------------------------------------------------------
    let blurred = out_dir.join("blurred.png");
    mcp.call_tool(
        "filter_image",
        json!({
            "input_path": cropped.display().to_string(),
            "output_path": blurred.display().to_string(),
            "filter": "gaussian-blur",
            "params": { "radius": 6.0 }
        }),
    );
    assert_eq!(dimensions(&blurred), (120, 90));
    let variance = |path: &Path| {
        let img = image::open(path).unwrap().into_luma8();
        let n = img.pixels().len() as f64;
        let mean = img.pixels().map(|p| p[0] as f64).sum::<f64>() / n;
        img.pixels()
            .map(|p| (p[0] as f64 - mean).powi(2))
            .sum::<f64>()
            / n
    };
    assert!(
        variance(&blurred) < variance(&cropped),
        "a blur should reduce local variation"
    );
    let unknown_filter = mcp.call_tool_expecting_failure(
        "filter_image",
        json!({
            "input_path": cropped.display().to_string(),
            "output_path": out_dir.join("x.png").display().to_string(),
            "filter": "instagram"
        }),
    );
    assert!(unknown_filter.contains("filter.gaussian-blur"), "{unknown_filter}");

    // --- run_operations: the escape hatch --------------------------------
    let composed = out_dir.join("composed.jpg");
    let ran = mcp.call_tool(
        "run_operations",
        json!({
            "input_path": photo("street.jpg"),
            "output_path": composed.display().to_string(),
            "operations": [
                { "op": "image.resize", "width": 640, "height": 480, "resample": "lanczos" },
                { "op": "select.rect", "x": 0, "y": 0, "width": 320, "height": 480 },
                { "op": "filter.gaussian-blur", "radius": 8.0 },
                { "op": "select.none" },
                { "op": "layer.add-adjustment", "adjustment": { "kind": "develop", "saturation": -100 } },
                { "op": "layer.flatten" }
            ],
            "quality": 85
        }),
    );
    assert_eq!(dimensions(&composed), (640, 480));
    assert_eq!(
        ran["structuredContent"]["operations"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    // Desaturated: the channels should agree everywhere.
    let flat = image::open(&composed).unwrap().into_rgb8();
    let px = flat.get_pixel(500, 240);
    assert!(
        px[0].abs_diff(px[1]) < 6 && px[1].abs_diff(px[2]) < 6,
        "saturation -100 should leave grey, got {px:?}"
    );
    // And the left half really is blurrier than the right.
    let half_variance = |x0: u32| {
        let img = image::open(&composed).unwrap().into_luma8();
        let vals: Vec<f64> = (x0..x0 + 300)
            .flat_map(|x| (0..480).map(move |y| (x, y)))
            .map(|(x, y)| img.get_pixel(x, y)[0] as f64)
            .collect();
        let mean = vals.iter().sum::<f64>() / vals.len() as f64;
        vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / vals.len() as f64
    };
    assert!(
        half_variance(10) < half_variance(330),
        "the selection should have confined the blur to the left half"
    );

    let bad_op = mcp.call_tool_expecting_failure(
        "run_operations",
        json!({
            "input_path": photo("street.jpg"),
            "output_path": out_dir.join("never.png").display().to_string(),
            "operations": [{ "op": "filter.deep_fry" }]
        }),
    );
    assert!(bad_op.contains("list_operations"), "{bad_op}");
    assert!(
        !out_dir.join("never.png").exists(),
        "a refused operation list must not write anything"
    );

    // --- the one allowed preview ------------------------------------------
    let with_preview = mcp.call_tool(
        "image_info",
        json!({ "path": photo("product.jpg"), "include_preview": true }),
    );
    let block = with_preview["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["type"] == "image")
        .expect("one image block");
    assert_eq!(block["mimeType"], "image/jpeg");
    let encoded = block["data"].as_str().unwrap();
    // Base64 of a JPEG small enough to be a preview and not a photograph.
    assert!(
        encoded.len() < 400_000,
        "the preview is {} base64 characters — that is not a preview",
        encoded.len()
    );
    let preview_meta = &with_preview["structuredContent"]["preview"];
    assert!(preview_meta["width"].as_u64().unwrap() <= 768);
    assert!(preview_meta["height"].as_u64().unwrap() <= 768);
    assert!(
        text_of(&with_preview).contains("Preview attached"),
        "the summary should say the preview is there and how big it is"
    );
    let capped = mcp.call_tool(
        "image_info",
        json!({ "path": photo("product.jpg"), "include_preview": true, "preview_max_side": 4000 }),
    );
    assert_eq!(
        capped["structuredContent"]["preview"]["width"]
            .as_u64()
            .unwrap()
            .max(capped["structuredContent"]["preview"]["height"].as_u64().unwrap()),
        768,
        "the cap must hold whatever is asked for"
    );

    // --- PSD --------------------------------------------------------------
    let psd = testdata()
        .join("psd/psd-tools/group.psd")
        .display()
        .to_string();
    let psd_info = mcp.call_tool("psd_info", json!({ "path": psd }));
    let psd_data = &psd_info["structuredContent"];
    assert!(
        psd_data["layer_count"].as_u64().unwrap() >= 2,
        "group.psd has layers"
    );
    assert!(
        psd_data["layer_tree"].is_array(),
        "psd_info should return the tree"
    );
    let tree_text = text_of(&psd_info);
    assert!(
        tree_text.contains("[group]") && tree_text.contains("layer(s)"),
        "psd_info should print the tree: {tree_text}"
    );

    let flattened = out_dir.join("group.png");
    mcp.call_tool(
        "psd_flatten",
        json!({ "input_path": psd, "output_path": flattened.display().to_string() }),
    );
    let (pw, ph) = dimensions(&flattened);
    assert_eq!(
        (pw as u64, ph as u64),
        (
            psd_data["width"].as_u64().unwrap(),
            psd_data["height"].as_u64().unwrap()
        ),
        "the flattened file should be the document's size"
    );

    let layers_dir = out_dir.join("layers");
    std::fs::create_dir(&layers_dir).unwrap();
    let exported = mcp.call_tool(
        "psd_export_layers",
        json!({ "input_path": psd, "output_dir": layers_dir.display().to_string() }),
    );
    let written = exported["structuredContent"]["layers"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!written.is_empty());
    for layer in &written {
        let path = PathBuf::from(layer["output_path"].as_str().unwrap());
        assert!(path.exists(), "{} was reported but not written", path.display());
        assert_eq!(dimensions(&path), (pw, ph), "layers are document-sized");
    }
    let not_a_psd = mcp.call_tool_expecting_failure(
        "psd_info",
        json!({ "path": photo("product.jpg") }),
    );
    assert!(not_a_psd.contains("image_info"), "{not_a_psd}");

    // --- RAW ---------------------------------------------------------------
    let raw = testdata()
        .join("raw/google-pixel-3a.dng")
        .display()
        .to_string();
    let developed = out_dir.join("developed.jpg");
    let raw_result = mcp.call_tool(
        "raw_develop",
        json!({
            "input_path": raw,
            "output_path": developed.display().to_string(),
            "quality": 88
        }),
    );
    let (dw, dh) = dimensions(&developed);
    assert!(dw > 1000 && dh > 1000, "{dw}×{dh} is not a developed RAW");
    assert_eq!(raw_result["structuredContent"]["width"].as_u64().unwrap(), dw as u64);
    let camera = &raw_result["structuredContent"]["camera"];
    assert!(
        camera["make"].as_str().unwrap_or("").len() > 2,
        "the camera should be named: {camera}"
    );
    // A developed frame is not a flat grey field.
    let developed_img = image::open(&developed).unwrap().into_luma8();
    let mean = developed_img.pixels().map(|p| p[0] as f64).sum::<f64>()
        / developed_img.pixels().len() as f64;
    assert!(mean > 5.0 && mean < 250.0, "mean luma {mean} looks wrong");

    // --- batch --------------------------------------------------------------
    let batch_dir = out_dir.join("batch");
    std::fs::create_dir(&batch_dir).unwrap();
    let batch = mcp.call_tool(
        "batch_edit",
        json!({
            "pattern": testdata().join("photos/*.jpg").display().to_string(),
            "output_dir": batch_dir.display().to_string(),
            "operations": [{ "op": "image.resize", "width": 200, "height": 200, "resample": "bilinear" }],
            "suffix": "-thumb",
            "format": "png"
        }),
    );
    let report = &batch["structuredContent"];
    let matched = report["matched"].as_u64().unwrap();
    assert!(matched >= 4, "testdata/photos has more than {matched} JPEGs");
    assert_eq!(report["failed"], 0, "{}", text_of(&batch));
    for file in report["files"].as_array().unwrap() {
        let path = PathBuf::from(file["output_path"].as_str().unwrap());
        assert_eq!(dimensions(&path), (200, 200));
    }

    // --- sessions -----------------------------------------------------------
    let opened = mcp.call_tool("open_document", json!({ "path": photo("old-bw.jpg") }));
    let id = opened["structuredContent"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (ow, oh) = (
        opened["structuredContent"]["width"].as_u64().unwrap(),
        opened["structuredContent"]["height"].as_u64().unwrap(),
    );
    assert_eq!(opened["structuredContent"]["saved"], true);

    let applied = mcp.call_tool(
        "apply_operations",
        json!({
            "session_id": id,
            "label": "Square crop and blur",
            "operations": [
                { "op": "image.crop", "x": 0, "y": 0, "width": 200, "height": 200 },
                { "op": "filter.gaussian-blur", "radius": 3.0 }
            ]
        }),
    );
    assert_eq!(applied["structuredContent"]["width"], 200);
    assert_eq!(applied["structuredContent"]["saved"], false);
    let undo_stack = applied["structuredContent"]["history"]["undo"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        undo_stack.len(),
        1,
        "a label should collapse both commands into one undo step: {undo_stack:?}"
    );
    assert_eq!(undo_stack[0], "Square crop and blur");

    let undone = mcp.call_tool("undo", json!({ "session_id": id }));
    assert_eq!(
        undone["structuredContent"]["width"].as_u64().unwrap(),
        ow,
        "undo should put the canvas back"
    );
    assert_eq!(undone["structuredContent"]["height"].as_u64().unwrap(), oh);
    let redone = mcp.call_tool("redo", json!({ "session_id": id }));
    assert_eq!(redone["structuredContent"]["width"], 200);

    let state = mcp.call_tool("document_state", json!({ "session_id": id }));
    assert_eq!(state["structuredContent"]["session_id"], id.as_str());
    assert!(text_of(&state).contains("unsaved changes"));

    // The viewer's own tool: this one is allowed to return pixels.
    let rendered = mcp.call_tool(
        "render_preview",
        json!({ "session_or_path": id, "max_side": 256 }),
    );
    let render_data = &rendered["structuredContent"];
    assert!(render_data["preview_data_uri"]
        .as_str()
        .unwrap()
        .starts_with("data:image/jpeg;base64,"));
    assert_eq!(render_data["session_id"], id.as_str());
    assert!(render_data["histogram"]["l"].is_array());
    assert!(render_data["document"]["width"].as_u64().unwrap() == 200);
    assert!(render_data["preview"]["width"].as_u64().unwrap() <= 256);

    // …and on a plain path, for a viewer that has no session.
    let rendered_path = mcp.call_tool(
        "render_preview",
        json!({ "session_or_path": photo("product.jpg"), "max_side": 200, "region": { "x": 0, "y": 0, "width": 100, "height": 100 } }),
    );
    assert_eq!(
        rendered_path["structuredContent"]["preview"]["width"],
        100,
        "a region smaller than max_side is not upscaled"
    );

    let saved_path = out_dir.join("session.psd");
    let saved = mcp.call_tool(
        "save_document",
        json!({
            "session_id": id,
            "output_path": saved_path.display().to_string()
        }),
    );
    assert!(saved_path.exists());
    assert_eq!(saved["structuredContent"]["format"], "psd");
    assert!(std::fs::metadata(&saved_path).unwrap().len() > 1000);
    // Round trip: the PSD we just wrote opens again at the right size.
    let reopened = mcp.call_tool("psd_info", json!({ "path": saved_path.display().to_string() }));
    assert_eq!(reopened["structuredContent"]["width"], 200);

    let state_after_save = mcp.call_tool("document_state", json!({ "session_id": id }));
    assert_eq!(state_after_save["structuredContent"]["saved"], true);

    let all_open = mcp.call_tool("document_state", json!({}));
    assert_eq!(
        all_open["structuredContent"]["open_documents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    mcp.call_tool("close_document", json!({ "session_id": id }));
    let gone = mcp.call_tool_expecting_failure(
        "document_state",
        json!({ "session_id": id }),
    );
    assert!(gone.contains("open_document"), "{gone}");

    // --- convert, with and without metadata --------------------------------
    let webp = out_dir.join("product.webp");
    let converted = mcp.call_tool(
        "convert_image",
        json!({
            "input_path": photo("product.jpg"),
            "output_path": webp.display().to_string()
        }),
    );
    assert!(webp.exists());
    assert_eq!(converted["structuredContent"]["format"], "webp");
    assert!(image::open(&webp).is_ok(), "the webp should decode");

    let kept = out_dir.join("product-copy.jpg");
    let copied = mcp.call_tool(
        "convert_image",
        json!({
            "input_path": photo("product.jpg"),
            "output_path": kept.display().to_string(),
            "metadata": "keep",
            "quality": 90
        }),
    );
    let said = copied["structuredContent"]["metadata"].as_str().unwrap();
    let carried = std::fs::read(&kept).unwrap().windows(4).any(|w| w == b"Exif");
    assert_eq!(
        said.contains("EXIF copied"),
        carried,
        "the report ({said}) must match what is in the file"
    );
    let bad_metadata = mcp.call_tool_expecting_failure(
        "convert_image",
        json!({
            "input_path": photo("product.jpg"),
            "output_path": out_dir.join("x2.jpg").display().to_string(),
            "metadata": "preserve"
        }),
    );
    assert!(bad_metadata.contains("strip"), "{bad_metadata}");

    // --- and an unknown tool is an error, not a silence ---------------------
    let unknown = mcp.raw_request(
        "tools/call",
        json!({ "name": "enhance", "arguments": {} }),
    );
    assert!(unknown.get("error").is_some() || unknown["result"]["isError"] == json!(true));
}
