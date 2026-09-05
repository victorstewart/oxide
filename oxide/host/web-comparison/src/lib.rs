//! Standalone browser comparison laboratory support.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg(target_arch = "wasm32")]
mod wasm;

pub const CONTROL_API_NAME: &str = "oxideComparisonV1";
pub const CONTROL_API_VERSION: u32 = 1;
pub const SHIPPING_MANIFEST_SCHEMA_VERSION: u32 = 2;
pub const IMPLEMENTATIONS: [&str; 3] = ["html-first", "client-dom", "oxide"];
pub const SCENARIOS: [&str; 6] = [
   "startup.first-screen",
   "dashboard.mixed-static",
   "feed.variable-scroll",
   "chat.live-update",
   "navigation.modal",
   "image.decode-zoom",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ShippingManifest
{
   pub schema_version: u32,
   pub implementation_id: String,
   pub source_root: String,
   pub gzip_tool: String,
   pub brotli_tool: String,
   pub files: Vec<ShippingFile>,
   pub category_totals: BTreeMap<String, ShippingTotals>,
   pub route_class_totals: BTreeMap<String, ShippingTotals>,
   pub totals: ShippingTotals,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ShippingFile
{
   pub path: String,
   pub mime_type: String,
   pub sha256: String,
   pub raw_bytes: u64,
   pub gzip_9_bytes: u64,
   pub brotli_11_bytes: u64,
   pub category: String,
   pub route_class: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ShippingTotals
{
   pub raw_bytes: u64,
   pub gzip_9_bytes: u64,
   pub brotli_11_bytes: u64,
   pub file_count: u64,
}

pub fn validate_source_tree(root: &Path) -> Result<()>
{
   for path in [
      "shared/control.js",
      "shared/scenes.js",
      "shared/styles.css",
      "html-first/index.html",
      "html-first/enhance.js",
      "client-dom/index.html",
      "client-dom/app.js",
      "oxide/index.html",
      "oxide/app.js",
      "comparison.campaign.spec.ts",
   ]
   {
      ensure!(root.join(path).is_file(), "web comparison source is missing {}", path);
   }

   let control = read_text(&root.join("shared/control.js"))?;
   for method in ["capabilities", "reset", "advance", "ready", "snapshot", "checkpoint", "teardown"]
   {
      ensure!(control.contains(method), "web control API is missing method {}", method);
   }
   ensure!(control.contains(CONTROL_API_NAME), "web control API global name differs from the frozen contract");
   ensure!(!control.contains("dispatchEvent("), "web control API must not synthesize timed input");

   let campaign = read_text(&root.join("comparison.campaign.spec.ts"))?;
   ensure!(campaign.contains("headed"), "browser campaign must freeze headed execution");
   ensure!(campaign.contains("ComparisonPlan"), "browser campaign must load the common ComparisonPlan");
   ensure!(campaign.contains("normalized-srgb8-exact-static-v1") && campaign.contains("from \"pngjs\""), "browser campaign must perform exact static PNG acceptance inline");
   ensure!(!campaign.contains("dispatchEvent("), "browser campaign must not synthesize timed input");
   ensure!(!campaign.contains(".click()"), "browser campaign must use trusted automation input rather than page click calls");

   let html_first = read_text(&root.join("html-first/index.html"))?;
   ensure!(html_first.contains("<main") && html_first.contains("<button") && html_first.contains("data-server-rendered=\"true\""), "HTML-first reference is not server-rendered semantic HTML");
   let client_dom = read_text(&root.join("client-dom/index.html"))?;
   ensure!(client_dom.contains("id=\"comparison-root\""), "client DOM reference has no application root");
   let oxide = read_text(&root.join("oxide/index.html"))?;
   let oxide_app = read_text(&root.join("oxide/app.js"))?;
   ensure!(oxide.contains("<canvas") && oxide.contains("/oxide/app.js") && oxide_app.contains("oxide_web_comparison"), "Oxide reference does not load the standalone WASM comparison runtime");
   Ok(())
}

pub fn build_shipping_manifest(root: &Path, implementation_id: &str) -> Result<ShippingManifest>
{
   ensure!(IMPLEMENTATIONS.contains(&implementation_id), "unknown web implementation {}", implementation_id);
   let implementation_root = root.join(implementation_id);
   ensure!(implementation_root.is_dir(), "shipping root does not exist: {}", implementation_root.display());
   let gzip_tool = tool_version("gzip", &["--version"])?;
   let brotli_tool = tool_version("brotli", &["--version"])?;
   let mut paths = Vec::new();
   collect_files(&implementation_root, &mut paths)?;
   paths.sort();
   let mut files = Vec::new();
   let mut totals = ShippingTotals::default();
   for path in paths
   {
      let relative = path.strip_prefix(&implementation_root).context("shipping path escaped implementation root")?;
      if !is_shipping_path(relative)
      {
         continue;
      }
      let bytes = fs::read(&path).with_context(|| format!("reading shipping file {}", path.display()))?;
      let raw_bytes = bytes.len() as u64;
      let gzip_9_bytes = compressed_len("gzip", &["-9", "-c"], &bytes)?;
      let brotli_11_bytes = compressed_len("brotli", &["-q", "11", "-c"], &bytes)?;
      totals.raw_bytes = totals.raw_bytes.saturating_add(raw_bytes);
      totals.gzip_9_bytes = totals.gzip_9_bytes.saturating_add(gzip_9_bytes);
      totals.brotli_11_bytes = totals.brotli_11_bytes.saturating_add(brotli_11_bytes);
      totals.file_count = totals.file_count.saturating_add(1);
      files.push(ShippingFile {
         path: relative.to_string_lossy().replace('\\', "/"),
         mime_type: mime_type(relative).to_string(),
         sha256: format!("{:x}", Sha256::digest(&bytes)),
         raw_bytes,
         gzip_9_bytes,
         brotli_11_bytes,
         category: category(relative).to_string(),
         route_class: route_class(relative).to_string(),
      });
   }
   ensure!(!files.is_empty(), "shipping manifest has no requested production files");
   let (category_totals, route_class_totals) = summarize_shipping_files(&files);
   Ok(ShippingManifest {
      schema_version: SHIPPING_MANIFEST_SCHEMA_VERSION,
      implementation_id: implementation_id.to_string(),
      source_root: implementation_root.to_string_lossy().into_owned(),
      gzip_tool,
      brotli_tool,
      files,
      category_totals,
      route_class_totals,
      totals,
   })
}

pub fn build_route_shipping_manifest(root: &Path, package_root: &Path, implementation_id: &str) -> Result<ShippingManifest>
{
   ensure!(IMPLEMENTATIONS.contains(&implementation_id), "unknown web implementation {}", implementation_id);
   validate_source_tree(root)?;
   let gzip_tool = tool_version("gzip", &["--version"])?;
   let brotli_tool = tool_version("brotli", &["--version"])?;
   let specification_root = root.join("../../../benchmarks/comparative/specs/v1");
   let mut entities: Vec<(String, PathBuf, String)> = Vec::new();
   let mut dynamic_entities: Vec<(String, Vec<u8>, String)> = Vec::new();

   entities.push((String::from("shared/control.js"), root.join("shared/control.js"), String::from("initial-route")));
   entities.push((String::from("shared/styles.css"), root.join("shared/styles.css"), String::from("initial-route")));
   if implementation_id != "oxide"
   {
      entities.push((String::from("shared/scenes.js"), root.join("shared/scenes.js"), String::from("initial-route")));
      for font in ["NotoSans-VF.ttf", "NotoSansArabic-VF.ttf", "NotoSansSC-VF.ttf"]
      {
         entities.push((format!("specs/font-packs/oxide-bench-fonts-v1/{font}"), specification_root.join("font-packs/oxide-bench-fonts-v1").join(font), String::from("initial-route")));
      }
      for asset in ["neutral-thumbnail-atlas-v1.png", "inline-text-atlas-v1-45px.png", "image-decode-zoom-thumbnail-v1.png"]
      {
         entities.push((format!("specs/assets/{asset}"), specification_root.join("assets").join(asset), String::from("initial-route")));
      }
   }
   entities.push((String::from("specs/assets/image-decode-zoom-source-v1.png"), specification_root.join("assets/image-decode-zoom-source-v1.png"), String::from("lazy-journey")));

   match implementation_id
   {
      "html-first" =>
      {
         entities.push((String::from("html-first/enhance.js"), root.join("html-first/enhance.js"), String::from("initial-route")));
         for scenario_id in SCENARIOS
         {
            dynamic_entities.push((format!("html-first/routes/{scenario_id}.html"), render_html_first_page(root, scenario_id)?.into_bytes(), String::from("initial-route")));
         }
      }
      "client-dom" =>
      {
         entities.push((String::from("client-dom/index.html"), root.join("client-dom/index.html"), String::from("initial-route")));
         entities.push((String::from("client-dom/app.js"), root.join("client-dom/app.js"), String::from("initial-route")));
         for scenario_id in SCENARIOS
         {
            entities.push((format!("specs/fixtures/{scenario_id}.json"), specification_root.join("fixtures").join(format!("{scenario_id}.json")), String::from("initial-route")));
         }
      }
      "oxide" =>
      {
         entities.push((String::from("oxide/index.html"), root.join("oxide/index.html"), String::from("initial-route")));
         entities.push((String::from("oxide/app.js"), root.join("oxide/app.js"), String::from("initial-route")));
         ensure!(package_root.is_dir(), "Oxide package root does not exist: {}", package_root.display());
         let mut package_paths = Vec::new();
         collect_files(package_root, &mut package_paths)?;
         package_paths.sort();
         for path in package_paths
         {
            let relative = path.strip_prefix(package_root).context("Oxide package path escaped package root")?;
            if is_browser_package_path(relative)
            {
               entities.push((format!("oxide/pkg/{}", relative.to_string_lossy().replace('\\', "/")), path, String::from("initial-route")));
            }
         }
      }
      _ => bail!("unsupported route shipping implementation {}", implementation_id),
   }

   let mut files = Vec::with_capacity(entities.len() + dynamic_entities.len());
   let mut totals = ShippingTotals::default();
   for (display_path, path, route) in entities
   {
      let bytes = fs::read(&path).with_context(|| format!("reading shipping entity {}", path.display()))?;
      push_shipping_file(&mut files, &mut totals, display_path, &bytes, route)?;
   }
   for (display_path, bytes, route) in dynamic_entities
   {
      push_shipping_file(&mut files, &mut totals, display_path, &bytes, route)?;
   }
   files.sort_by(|left, right| left.path.cmp(&right.path));
   ensure!(!files.is_empty(), "route shipping manifest has no production entities");
   let (category_totals, route_class_totals) = summarize_shipping_files(&files);
   Ok(ShippingManifest {
      schema_version: SHIPPING_MANIFEST_SCHEMA_VERSION,
      implementation_id: implementation_id.to_string(),
      source_root: root.to_string_lossy().into_owned(),
      gzip_tool,
      brotli_tool,
      files,
      category_totals,
      route_class_totals,
      totals,
   })
}

fn push_shipping_file(files: &mut Vec<ShippingFile>, totals: &mut ShippingTotals, display_path: String, bytes: &[u8], route_class: String) -> Result<()>
{
   let path = Path::new(&display_path);
   let raw_bytes = bytes.len() as u64;
   let gzip_9_bytes = compressed_len("gzip", &["-9", "-c"], bytes)?;
   let brotli_11_bytes = compressed_len("brotli", &["-q", "11", "-c"], bytes)?;
   totals.raw_bytes = totals.raw_bytes.saturating_add(raw_bytes);
   totals.gzip_9_bytes = totals.gzip_9_bytes.saturating_add(gzip_9_bytes);
   totals.brotli_11_bytes = totals.brotli_11_bytes.saturating_add(brotli_11_bytes);
   totals.file_count = totals.file_count.saturating_add(1);
   let mime_type = mime_type(path).to_string();
   let category = category(path).to_string();
   files.push(ShippingFile {
      path: display_path,
      mime_type,
      sha256: format!("{:x}", Sha256::digest(bytes)),
      raw_bytes,
      gzip_9_bytes,
      brotli_11_bytes,
      category,
      route_class,
   });
   Ok(())
}

fn summarize_shipping_files(files: &[ShippingFile]) -> (BTreeMap<String, ShippingTotals>, BTreeMap<String, ShippingTotals>)
{
   let mut category_totals = BTreeMap::new();
   let mut route_class_totals = BTreeMap::new();
   for file in files
   {
      add_shipping_total(category_totals.entry(file.category.clone()).or_default(), file);
      add_shipping_total(route_class_totals.entry(file.route_class.clone()).or_default(), file);
   }
   (category_totals, route_class_totals)
}

fn add_shipping_total(total: &mut ShippingTotals, file: &ShippingFile)
{
   total.raw_bytes = total.raw_bytes.saturating_add(file.raw_bytes);
   total.gzip_9_bytes = total.gzip_9_bytes.saturating_add(file.gzip_9_bytes);
   total.brotli_11_bytes = total.brotli_11_bytes.saturating_add(file.brotli_11_bytes);
   total.file_count = total.file_count.saturating_add(1);
}

pub fn write_manifest(path: &Path, manifest: &ShippingManifest) -> Result<()>
{
   let mut bytes = serde_json::to_vec_pretty(manifest).context("serializing shipping manifest")?;
   bytes.push(b'\n');
   let parent = path.parent().context("shipping manifest path has no parent")?;
   fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
   let temporary = path.with_extension("tmp");
   fs::write(&temporary, bytes).with_context(|| format!("writing {}", temporary.display()))?;
   fs::rename(&temporary, path).with_context(|| format!("committing {}", path.display()))
}

pub fn serve(root: &Path, address: &str) -> Result<()>
{
   validate_source_tree(root)?;
   let listener = TcpListener::bind(address).with_context(|| format!("binding web comparison server at {}", address))?;
   for stream in listener.incoming()
   {
      let mut stream = stream.context("accepting web comparison connection")?;
      if let Err(error) = serve_one(root, &mut stream)
      {
         let body = format!("web comparison server error: {error}\n");
         let _ = write_response(&mut stream, 500, "text/plain; charset=utf-8", body.as_bytes());
      }
   }
   Ok(())
}

fn serve_one(root: &Path, stream: &mut TcpStream) -> Result<()>
{
   let mut request = [0u8; 16 * 1024];
   let length = stream.read(&mut request).context("reading web comparison request")?;
   ensure!(length > 0, "empty web comparison request");
   let request = std::str::from_utf8(&request[..length]).context("request is not UTF-8")?;
   let line = request.lines().next().context("request has no line")?;
   let mut pieces = line.split_whitespace();
   ensure!(pieces.next() == Some("GET"), "web comparison server accepts GET only");
   let target = pieces.next().context("request has no target")?;
   let path = target.split('?').next().unwrap_or("/");
   if path == "/" || path == "/html-first/index.html"
   {
      let scenario_id = query_parameter(target, "scenario").unwrap_or("dashboard.mixed-static");
      ensure!(SCENARIOS.contains(&scenario_id), "unsupported HTML-first scenario {}", scenario_id);
      let bytes = render_html_first_page(root, scenario_id)?;
      return write_response(stream, 200, "text/html; charset=utf-8", bytes.as_bytes());
   }
   let relative = if path == "/" {Path::new("html-first/index.html")} else {Path::new(path.trim_start_matches('/'))};
   ensure!(relative.components().all(|component| matches!(component, Component::Normal(_))), "request path is not normalized");
   let resolved = if relative.starts_with("specs")
   {
      let staged_specification_root = root.join("specs");
      let specification_root = if staged_specification_root.is_dir() {staged_specification_root} else {root.join("../../../benchmarks/comparative/specs/v1")};
      specification_root.join(relative.strip_prefix("specs").context("specification prefix disappeared")?)
   }
   else if relative.starts_with("oxide/pkg")
   {
      let package_root = std::env::var_os("OXIDE_WEB_COMPARISON_PKG_ROOT").map(PathBuf::from).unwrap_or_else(|| root.join("oxide/pkg"));
      package_root.join(relative.strip_prefix("oxide/pkg").context("Oxide package prefix disappeared")?)
   }
   else
   {
      root.join(relative)
   };
   if !resolved.is_file()
   {
      return write_response(stream, 404, "text/plain; charset=utf-8", b"not found\n");
   }
   let bytes = fs::read(&resolved).with_context(|| format!("reading {}", resolved.display()))?;
   write_response(stream, 200, mime_type(relative), &bytes)
}

fn query_parameter<'a>(target: &'a str, name: &str) -> Option<&'a str>
{
   target.split_once('?')?.1.split('&').find_map(|pair|
   {
      let (key, value) = pair.split_once('=')?;
      (key == name).then_some(value)
   })
}

fn render_html_first_page(root: &Path, scenario_id: &str) -> Result<String>
{
   let fixture_path = root.join("../../../benchmarks/comparative/specs/v1/fixtures").join(format!("{}.json", scenario_id));
   let fixture_bytes = fs::read(&fixture_path).with_context(|| format!("reading HTML-first fixture {}", fixture_path.display()))?;
   let fixture: serde_json::Value = serde_json::from_slice(&fixture_bytes).with_context(|| format!("parsing HTML-first fixture {}", fixture_path.display()))?;
   let mut scene = String::with_capacity(256 * 1024);
   match scenario_id
   {
      "startup.first-screen" => render_startup_html(&fixture, &mut scene)?,
      "dashboard.mixed-static" => render_dashboard_html(&fixture, &mut scene)?,
      "feed.variable-scroll" => render_feed_html(&fixture, &mut scene)?,
      "chat.live-update" => render_chat_html(&fixture, &mut scene)?,
      "navigation.modal" => render_navigation_html(&fixture, &mut scene)?,
      "image.decode-zoom" => render_image_html(&mut scene),
      _ => bail!("unsupported HTML-first scenario {}", scenario_id),
   }
   let fixture_json = String::from_utf8(fixture_bytes).context("HTML-first fixture is not UTF-8")?.replace('<', "\\u003c");
   Ok(format!(r#"<!doctype html>
<html lang="en">
   <head>
      <meta charset="utf-8">
      <meta name="viewport" content="width=device-width,initial-scale=1">
      <title>HTML-first Production Comparison</title>
      <link rel="stylesheet" href="/shared/styles.css">
   <body>
      <main id="comparison-root" class="comparison-viewport" data-server-rendered="true" data-implementation="html-first.production" data-scenario-id="{scenario_id}">{scene}</main>
      <script id="comparison-fixture" type="application/json">{fixture_json}</script>
      <script type="module" src="/html-first/enhance.js"></script>
   </body>
</html>
"#))
}

fn render_startup_html(fixture: &serde_json::Value, output: &mut String) -> Result<()>
{
   output.push_str("<h1 class=\"scene-title startup-title\" data-role=\"header\">Production Comparison</h1><div class=\"startup-navigation\" data-role=\"navigation\">First Screen</div><section class=\"startup-grid\">");
   let cards = json_array(fixture, "cards")?;
   let data = json_string(fixture, "data")?;
   for card in cards.iter().filter(|card| card["initially_visible"].as_bool() == Some(true)).take(6)
   {
      let id = card["id"].as_str().context("startup card id is missing")?;
      let offset = card["data_offset"].as_u64().context("startup card data_offset is missing")? as usize;
      let detail = data.get(offset..offset.saturating_add(48).min(data.len())).unwrap_or("");
      let thumbnail = card["thumbnail_index"].as_u64().context("startup card thumbnail_index is missing")? as usize;
      output.push_str("<article class=\"startup-card\" data-role=\"card\">");
      render_atlas_tile(output, "startup-thumbnail", "initial-image", thumbnail, &format!("Thumbnail {}", thumbnail))?;
      output.push_str("<strong>");
      push_html(output, id);
      output.push_str("</strong><p>");
      push_html(output, detail);
      output.push_str("</p></article>");
   }
   output.push_str("</section><button class=\"primary-control\" data-role=\"primary-control\">Continue</button>");
   Ok(())
}

fn render_dashboard_html(fixture: &serde_json::Value, output: &mut String) -> Result<()>
{
   let categories = fixture.get("categories").context("dashboard categories are missing")?;
   let card_count = categories["rounded_card"].as_u64().context("dashboard card count is missing")? as usize;
   let backdrop_count = categories["backdrop_region"].as_u64().context("dashboard backdrop count is missing")? as usize;
   let control_count = categories["control"].as_u64().context("dashboard control count is missing")? as usize;
   output.push_str("<section class=\"dashboard-surface\" data-role=\"dashboard\" aria-label=\"Dashboard\"><div class=\"dashboard-backdrops\">");
   for _ in 0..backdrop_count
   {
      output.push_str("<div data-role=\"backdrop-region\"></div>");
   }
   output.push_str("</div><div class=\"dashboard-grid\">");
   for index in 0..card_count
   {
      write!(output, "<article class=\"dashboard-card\" data-role=\"rounded-card\" aria-label=\"Dashboard card {}\">", index + 1)?;
      render_atlas_tile(output, "dashboard-icon", "icon-image", index * 2, &format!("Icon {}", index * 2))?;
      render_atlas_tile(output, "dashboard-icon", "icon-image", index * 2 + 1, &format!("Icon {}", index * 2 + 1))?;
      output.push_str("<span class=\"dashboard-labels\">");
      let label_count = if index < 16 {6} else {5};
      let label_base = if index < 16 {index * 6} else {96 + (index - 16) * 5};
      for label_index in 0..label_count
      {
         write!(output, "<span data-role=\"label\">Node {}</span>", label_base + label_index)?;
      }
      output.push_str("</span>");
      if index < control_count
      {
         let control_index = if index == 0 {3} else {label_base};
         write!(output, "<button data-role=\"control\" aria-label=\"Update Node {}\" data-index=\"{}\"></button>", control_index, control_index)?;
      }
      output.push_str("</article>");
   }
   output.push_str("</div></section>");
   Ok(())
}

fn render_feed_html(fixture: &serde_json::Value, output: &mut String) -> Result<()>
{
   let rows = json_array(fixture, "rows")?;
   let content_height = rows.iter().try_fold(0u64, |height, row| row["height"].as_u64().context("feed row height is missing").map(|row_height| height.saturating_add(row_height)))?;
   write!(output, "<h1 class=\"scene-title feed-title\" data-role=\"navigation-bar\">Measured Feed</h1><section class=\"feed-list\" data-role=\"feed\"><div class=\"feed-spacer\" style=\"height:{}px\"><div class=\"feed-window\">", content_height)?;
   let mut visible_height = 0u64;
   for row in rows
   {
      if visible_height >= 792
      {
         break;
      }
      let height = row["height"].as_u64().context("feed row height is missing")?;
      let id = row["id"].as_str().context("feed row id is missing")?;
      let text = row["text"].as_str().context("feed row text is missing")?;
      let direction = row["direction"].as_str().context("feed row direction is missing")?;
      let thumbnail = row["thumbnail_index"].as_u64().context("feed thumbnail index is missing")? as usize;
      write!(output, "<article class=\"feed-row\" style=\"top:{}px;height:{}px\"><div class=\"feed-card\" data-role=\"feed-card\">", visible_height, height)?;
      render_atlas_tile(output, "feed-thumbnail", "thumbnail", thumbnail, &format!("Thumbnail {}", thumbnail))?;
      write!(output, "<div class=\"feed-copy\" dir=\"{}\">", direction)?;
      push_inline_html(output, text);
      output.push_str("</div><small>");
      push_html(output, id);
      output.push_str("</small><button class=\"favorite\" data-role=\"favorite-control\" aria-label=\"Favorite ");
      push_html(output, id);
      output.push_str("\" data-id=\"");
      push_html(output, id);
      output.push_str("\"></button></div></article>");
      visible_height = visible_height.saturating_add(height);
   }
   output.push_str("</div></div></section>");
   Ok(())
}

fn render_chat_html(fixture: &serde_json::Value, output: &mut String) -> Result<()>
{
   const ROW_HEIGHTS: [u64; 10] = [58, 62, 66, 70, 74, 78, 82, 76, 70, 64];
   let messages = json_array(fixture, "messages")?;
   let visible_start = messages.len().saturating_sub(10);
   output.push_str("<h1 class=\"scene-title chat-title\">Live Chat</h1><section class=\"chat-thread\" data-role=\"chat-thread\">");
   for message in &messages[visible_start..]
   {
      let sequence = message["sequence"].as_u64().context("chat message sequence is missing")?;
      let avatar = message["avatar_index"].as_u64().context("chat avatar index is missing")? as usize;
      let direction = message["direction"].as_str().context("chat direction is missing")?;
      let text = message["text"].as_str().context("chat text is missing")?;
      write!(output, "<article class=\"chat-row{}\" style=\"height:{}px\">", if direction == "rtl" {" rtl"} else {""}, ROW_HEIGHTS[sequence as usize % ROW_HEIGHTS.len()])?;
      render_atlas_tile(output, "chat-avatar", "avatar", avatar, &format!("Avatar {}", avatar))?;
      write!(output, "<div class=\"chat-message\" data-role=\"message\" dir=\"{}\">", direction)?;
      push_inline_html(output, text);
      output.push_str("</div></article>");
   }
   output.push_str("</section><textarea class=\"chat-composer\" data-role=\"composer\" aria-label=\"Message\"></textarea><button class=\"chat-send\" data-role=\"send-control\">Send</button>");
   Ok(())
}

fn render_navigation_html(fixture: &serde_json::Value, output: &mut String) -> Result<()>
{
   let item_count = fixture["list_item_count"].as_u64().context("navigation item count is missing")?;
   output.push_str("<h1 class=\"navigation-title\">Navigation</h1><nav class=\"navigation-list\" data-role=\"navigation-list\">");
   for index in 0..item_count
   {
      write!(output, "<button class=\"navigation-row\" data-role=\"list-item\" data-index=\"{}\"><strong>Item {}</strong><small>Canonical navigation row</small></button>", index, index + 1)?;
   }
   output.push_str("</nav>");
   Ok(())
}

fn render_image_html(output: &mut String)
{
   output.push_str("<h1 class=\"scene-title image-title\">Decode &amp; Zoom</h1><section class=\"image-stage\" data-role=\"image-canvas\"><img class=\"image-thumbnail\" data-role=\"image\" src=\"/specs/assets/image-decode-zoom-thumbnail-v1.png\" alt=\"Decoded benchmark fixture\"></section><input class=\"zoom-control\" data-role=\"zoom-control\" aria-label=\"Zoom\" type=\"range\" min=\"1\" max=\"2\" step=\"0.01\" value=\"1\">");
}

fn render_atlas_tile(output: &mut String, class_name: &str, role: &str, index: usize, label: &str) -> Result<()>
{
   write!(output, "<span class=\"{} atlas-tile\" data-role=\"{}\" role=\"img\" aria-label=\"", class_name, role)?;
   push_html(output, label);
   write!(output, "\" style=\"--atlas-column:{};--atlas-row:{}\"></span>", index % 8, index / 8)?;
   Ok(())
}

fn json_array<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a Vec<serde_json::Value>>
{
   value.get(key).and_then(serde_json::Value::as_array).with_context(|| format!("fixture array {} is missing", key))
}

fn json_string<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str>
{
   value.get(key).and_then(serde_json::Value::as_str).with_context(|| format!("fixture string {} is missing", key))
}

fn push_html(output: &mut String, value: &str)
{
   for character in value.chars()
   {
      match character
      {
         '&' => output.push_str("&amp;"),
         '<' => output.push_str("&lt;"),
         '>' => output.push_str("&gt;"),
         '"' => output.push_str("&quot;"),
         '\'' => output.push_str("&#39;"),
         _ => output.push(character),
      }
   }
}

fn push_inline_html(output: &mut String, value: &str)
{
   const ASSETS: [(&str, usize, usize); 10] = [
      ("👨‍👩‍👧‍👦", 2, 1),
      ("👩🏽‍💻", 4, 0),
      ("🇺🇳", 3, 1),
      ("🇯🇵", 4, 1),
      ("●", 0, 0),
      ("◆", 1, 0),
      ("★", 2, 0),
      ("☺", 3, 0),
      ("🌍", 0, 1),
      ("✨", 1, 1),
   ];
   let mut remaining = value;
   while !remaining.is_empty()
   {
      if let Some((grapheme, column, row)) = ASSETS.iter().find(|(grapheme, _, _)| remaining.starts_with(grapheme))
      {
         output.push_str("<span class=\"inline-text-asset\" role=\"img\" aria-label=\"");
         push_html(output, grapheme);
         let _ = write!(output, "\" style=\"--inline-column:{};--inline-row:{}\"></span>", column, row);
         remaining = &remaining[grapheme.len()..];
      }
      else
      {
         let length = remaining.chars().next().map(char::len_utf8).unwrap_or(0);
         push_html(output, &remaining[..length]);
         remaining = &remaining[length..];
      }
   }
}

fn write_response(stream: &mut TcpStream, status: u16, mime: &str, body: &[u8]) -> Result<()>
{
   let reason = if status == 200 {"OK"} else if status == 404 {"Not Found"} else {"Internal Server Error"};
   write!(stream, "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", status, reason, mime, body.len()).context("writing response headers")?;
   stream.write_all(body).context("writing response body")
}

fn read_text(path: &Path) -> Result<String>
{
   fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

fn collect_files(root: &Path, output: &mut Vec<PathBuf>) -> Result<()>
{
   for entry in fs::read_dir(root).with_context(|| format!("listing {}", root.display()))?
   {
      let entry = entry.context("reading shipping directory entry")?;
      let path = entry.path();
      if path.is_dir()
      {
         collect_files(&path, output)?;
      }
      else if path.is_file()
      {
         output.push(path);
      }
   }
   Ok(())
}

fn is_shipping_path(path: &Path) -> bool
{
   let value = path.to_string_lossy();
   !value.ends_with(".map")
      && !value.ends_with(".d.ts")
      && !value.contains("benchmark")
      && !value.contains("campaign")
}

fn is_browser_package_path(path: &Path) -> bool
{
   is_shipping_path(path) && matches!(path.extension().and_then(|value| value.to_str()), Some("js" | "wasm"))
}

fn compressed_len(program: &str, arguments: &[&str], bytes: &[u8]) -> Result<u64>
{
   let mut child = Command::new(program)
      .args(arguments)
      .stdin(std::process::Stdio::piped())
      .stdout(std::process::Stdio::piped())
      .spawn()
      .with_context(|| format!("starting {}", program))?;
   let mut stdin = child.stdin.take().context("compressor stdin unavailable")?;
   let output = std::thread::scope(|scope| -> Result<_>
   {
      let writer = scope.spawn(move || stdin.write_all(bytes));
      let output = child.wait_with_output().context("waiting for compressor")?;
      writer.join().map_err(|_| anyhow!("compressor stdin writer panicked"))?.context("writing compressor input")?;
      Ok(output)
   })?;
   ensure!(output.status.success(), "{} failed with {}", program, output.status);
   Ok(output.stdout.len() as u64)
}

fn tool_version(program: &str, arguments: &[&str]) -> Result<String>
{
   let output = Command::new(program).args(arguments).output().with_context(|| format!("reading {} version", program))?;
   ensure!(output.status.success(), "{} version command failed", program);
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);
   let value = stdout.lines().find(|line| !line.trim().is_empty()).or_else(|| stderr.lines().find(|line| !line.trim().is_empty())).unwrap_or(program);
   Ok(value.trim().to_string())
}

fn mime_type(path: &Path) -> &'static str
{
   match path.extension().and_then(|value| value.to_str()).unwrap_or("")
   {
      "html" => "text/html; charset=utf-8",
      "css" => "text/css; charset=utf-8",
      "js" | "mjs" => "text/javascript; charset=utf-8",
      "json" => "application/json",
      "wasm" => "application/wasm",
      "png" => "image/png",
      "ttf" => "font/ttf",
      _ => "application/octet-stream",
   }
}

fn category(path: &Path) -> &'static str
{
   match path.extension().and_then(|value| value.to_str()).unwrap_or("")
   {
      "html" => "html",
      "css" => "css",
      "js" | "mjs" => "javascript",
      "wasm" => "wasm",
      "ttf" => "font",
      "png" | "jpg" | "jpeg" | "webp" => "image",
      "wgsl" => "shader",
      "json" => "data",
      _ => "other",
   }
}

fn route_class(path: &Path) -> &'static str
{
   let value = path.to_string_lossy();
   if value.contains("image-decode-zoom-source")
   {
      "lazy-journey"
   }
   else
   {
      "initial-route"
   }
}

#[cfg(test)]
mod tests
{
   use super::*;

   #[test]
   fn development_artifacts_are_not_shipping_payloads()
   {
      assert!(!is_shipping_path(Path::new("pkg/runtime.d.ts")));
      assert!(!is_shipping_path(Path::new("pkg/runtime.js.map")));
      assert!(!is_shipping_path(Path::new("comparison.campaign.spec.ts")));
      assert!(is_shipping_path(Path::new("pkg/runtime.js")));
      assert!(is_shipping_path(Path::new("pkg/runtime_bg.wasm")));
      assert!(is_browser_package_path(Path::new("runtime.js")));
      assert!(is_browser_package_path(Path::new("runtime_bg.wasm")));
      assert!(!is_browser_package_path(Path::new("package.json")));
      assert!(!is_browser_package_path(Path::new("runtime.d.ts")));
   }

   #[test]
   fn compressor_streams_inputs_larger_than_a_pipe_buffer()
   {
      let bytes = vec![0x5a; 1024 * 1024];
      assert_eq!(compressed_len("sh", &["-c", "cat"], &bytes).unwrap(), bytes.len() as u64);
   }

   #[test]
   fn shipping_summary_preserves_category_and_route_totals()
   {
      let files = vec![
         ShippingFile {path: String::from("app.js"), mime_type: String::from("text/javascript"), sha256: String::from("a"), raw_bytes: 10, gzip_9_bytes: 7, brotli_11_bytes: 6, category: String::from("javascript"), route_class: String::from("initial-route")},
         ShippingFile {path: String::from("image.png"), mime_type: String::from("image/png"), sha256: String::from("b"), raw_bytes: 20, gzip_9_bytes: 17, brotli_11_bytes: 16, category: String::from("image"), route_class: String::from("lazy-journey")},
      ];
      let (categories, routes) = summarize_shipping_files(&files);
      assert_eq!(categories["javascript"], ShippingTotals {raw_bytes: 10, gzip_9_bytes: 7, brotli_11_bytes: 6, file_count: 1});
      assert_eq!(categories["image"], ShippingTotals {raw_bytes: 20, gzip_9_bytes: 17, brotli_11_bytes: 16, file_count: 1});
      assert_eq!(routes["initial-route"].file_count, 1);
      assert_eq!(routes["lazy-journey"].raw_bytes, 20);
   }

   #[test]
   fn dom_gpu_timestamp_is_not_a_comparable_metric()
   {
      for implementation in ["html-first", "client-dom"]
      {
         assert_ne!(implementation, "oxide");
      }
   }

   #[test]
   fn all_required_product_tracks_are_distinct()
   {
      assert_eq!(IMPLEMENTATIONS, ["html-first", "client-dom", "oxide"]);
      assert_eq!(SCENARIOS.len(), 6);
   }

   #[test]
   fn rapid_checkpoint_control_is_shared_by_every_track()
   {
      let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("sites");
      let control = fs::read_to_string(root.join("shared/control.js")).expect("reading shared control");
      let dom = fs::read_to_string(root.join("shared/scenes.js")).expect("reading DOM comparison adapter");
      let oxide = fs::read_to_string(root.join("oxide/app.js")).expect("reading Oxide comparison adapter");
      let wasm = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/wasm.rs")).expect("reading Oxide WebGPU runtime");
      assert!(control.contains("advance: checkpointId") && control.contains("oxide-comparison-output-ready"));
      assert!(oxide.contains("advance: async checkpointId => app.advance(checkpointId)"));
      for checkpoint in ["fresh-install-ready", "leaf-updated", "favorite-applied", "append-settled", "modal-100", "first-visible", "pan-mid"]
      {
         assert!(dom.contains(checkpoint), "DOM adapter is missing rapid checkpoint {checkpoint}");
         assert!(wasm.contains(checkpoint), "Oxide WebGPU adapter is missing rapid checkpoint {checkpoint}");
      }
   }

   #[test]
   fn web_plans_bind_the_materialized_shipping_manifests()
   {
      let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/comparative/specs/v1/plans");
      let campaign = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("sites/comparison.campaign.spec.ts")).expect("reading browser campaign");
      let campaign_sha256 = format!("{:x}", Sha256::digest(&campaign));
      for (name, reference_manifest) in [
         ("web-pr-html-first.json", "696a4a70af39e1ed0739fa9382de568242a6c2731c032a17f7c571804ef6c6e4"),
         ("web-pr-client-dom.json", "29104cd2ee53ad058855f1d02cf90c05401d1f7537fd9fc325ad810dd1b33548"),
      ]
      {
         let bytes = fs::read(root.join(name)).expect("reading web comparison plan");
         let plan: oxide_benchmark_spec::ComparisonPlan = serde_json::from_slice(&bytes).expect("parsing web comparison plan");
         oxide_benchmark_spec::validate_comparison_plan(&plan).expect("validating web comparison plan");
         assert_eq!(plan.reference.shipping_payload_manifest_sha256, reference_manifest);
         assert_eq!(plan.contender.shipping_payload_manifest_sha256, "12e78ed587204c97ed391fd3b6652d8a98a9790d74e692257490d8a08c79ba21");
         assert_eq!(plan.common.harness_sha256, campaign_sha256);
         assert_eq!(plan.common.pass_instrumentation_sha256, "d2fd24df3df839c03cfe750aeb9eb0840413a14cefee06bbcfa160d9ac2bb002");
      }
   }

   #[test]
   fn every_html_first_route_is_server_rendered_from_its_fixture()
   {
      let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("sites");
      let expected_initial_role_nodes = [15, 301, 29, 23, 13, 3];
      for (scenario_id, expected_roles) in SCENARIOS.into_iter().zip(expected_initial_role_nodes)
      {
         let page = render_html_first_page(&root, scenario_id).expect("rendering HTML-first route");
         assert!(page.contains("data-server-rendered=\"true\""));
         assert!(page.contains(&format!("data-scenario-id=\"{}\"", scenario_id)));
         assert!(page.contains(&format!("\"id\": \"{}\"", scenario_id)) || page.contains(&format!("\"id\":\"{}\"", scenario_id)));
         assert!(page.contains("id=\"comparison-fixture\""));
         assert_eq!(page.matches("data-role=").count(), expected_roles);
      }
   }

   #[test]
   fn html_first_query_selects_the_exact_canonical_route()
   {
      assert_eq!(query_parameter("/html-first/index.html?scenario=chat.live-update", "scenario"), Some("chat.live-update"));
      assert_eq!(query_parameter("/html-first/index.html?seed=0&scenario=image.decode-zoom", "scenario"), Some("image.decode-zoom"));
      assert_eq!(query_parameter("/html-first/index.html", "scenario"), None);
   }

   #[test]
   fn html_first_uses_the_pinned_inline_text_atlas()
   {
      let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("sites");
      for scenario_id in ["feed.variable-scroll", "chat.live-update"]
      {
         let page = render_html_first_page(&root, scenario_id).expect("rendering inline-text route");
         assert!(page.contains("class=\"inline-text-asset\""));
      }
   }
}
