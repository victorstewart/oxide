#!/usr/bin/env node

import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { readFileSync, statSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";

const root = process.argv[2];
const samples = process.argv[3] || "3";
const frames = process.argv[4] || "24";
const dirty = process.argv[5] === "1";
const flat = process.argv[6] === "1";
const threshold = process.argv[7] || "8";
const output = process.argv[8] || "";
const guardrails = process.argv[9] === "1";
const raf = process.argv[10] === "1";
if (!root) {
   throw new Error("usage: run_webgpu_prepared_c25.mjs WEB_ROOT [SAMPLES] [FRAMES] [DIRTY] [FLAT] [THRESHOLD] [OUTPUT] [GUARDRAILS] [RAF]");
}

const benchHtml = `<!doctype html>
<html><body><canvas id="oxide-canvas"></canvas><script type="module">
import init, { OxideWebApp } from "./pkg/oxide_host_web.js";
const params = new URLSearchParams(window.location.search);
try {
   await init();
   const app = await OxideWebApp.newAsync("oxide-canvas");
   app.prewarm_webgpu_bench_resources();
   const nextAnimationFrame = () => new Promise(resolve => requestAnimationFrame(resolve));
   const runPreparedRaf = async () => {
      const warmupSamples = [];
      let previous = await nextAnimationFrame();
      app.render_webgpu_prepared_snapshot(params.get("flat") === "1");
      for (let warmup = 1; warmup < 64; warmup += 1) {
         const timestamp = await nextAnimationFrame();
         warmupSamples.push(timestamp - previous);
         app.render_webgpu_prepared_snapshot(params.get("flat") === "1");
         previous = timestamp;
      }
      const frameSamples = [];
      const cpuSamples = [];
      const frameCount = Number(params.get("frames"));
      for (let frame = 0; frame < frameCount; frame += 1) {
         const timestamp = await nextAnimationFrame();
         const start = performance.now();
         app.render_webgpu_prepared_snapshot(params.get("flat") === "1");
         cpuSamples.push(performance.now() - start);
         frameSamples.push(timestamp - previous);
         previous = timestamp;
      }
      return "raf_frames=" + frameCount
         + ";raf_warmup_samples_ms=" + warmupSamples.join(",")
         + ";raf_frame_samples_ms=" + frameSamples.join(",")
         + ";raf_cpu_samples_ms=" + cpuSamples.join(",");
   };
   const result = params.get("guardrails") === "1"
      ? await app.bench_webgpu_prepared_guardrails()
      : params.get("raf") === "1"
      ? await runPreparedRaf()
      : await app.bench_webgpu_prepared_chunks(
         Number(params.get("samples")),
         Number(params.get("frames")),
         params.get("dirty") === "1",
         params.get("flat") === "1",
         Number(params.get("threshold")),
      );
   await fetch("/__c25_result", { method: "POST", body: result });
} catch (error) {
   await fetch("/__c25_result", { method: "POST", body: "error=" + (error?.stack || error) });
}
</script></body></html>`;

const mime = path => extname(path) === ".wasm" ? "application/wasm" : extname(path) === ".js" ? "text/javascript" : "text/html";
let resolveResult;
let rejectResult;
const result = new Promise((resolve, reject) => {
   resolveResult = resolve;
   rejectResult = reject;
});
const server = createServer((request, response) => {
   if (request.method === "POST" && request.url === "/__c25_result") {
      let body = "";
      request.on("data", chunk => body += chunk);
      request.on("end", () => {
         response.writeHead(204);
         response.end();
         resolveResult(body);
      });
      request.on("error", rejectResult);
      return;
   }
   if (request.url.split("?", 1)[0] === "/c25-prepared-bench.html") {
      response.writeHead(200, {
         "Content-Type": "text/html",
         "Cross-Origin-Opener-Policy": "same-origin",
         "Cross-Origin-Embedder-Policy": "require-corp",
      });
      response.end(benchHtml);
      return;
   }
   const relative = request.url.split("?", 1)[0] === "/" ? "/c25-prepared-bench.html" : request.url.split("?", 1)[0];
   if (relative.includes("..")) {
      response.writeHead(400);
      response.end();
      return;
   }
   try {
      const path = join(root, relative);
      if (!statSync(path).isFile()) {
         throw new Error("not a file");
      }
      response.writeHead(200, {
         "Content-Type": mime(path),
         "Cross-Origin-Opener-Policy": "same-origin",
         "Cross-Origin-Embedder-Policy": "require-corp",
      });
      response.end(readFileSync(path));
   } catch (_error) {
      response.writeHead(404);
      response.end();
   }
});
await new Promise((resolve, reject) => {
   server.once("error", reject);
   server.listen(0, "127.0.0.1", resolve);
});

const port = server.address().port;
const chrome = process.env.CHROME_BIN || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const chromeArch = process.env.CHROME_ARCH || "";
const chromeArgs = [
   "--headless=new",
   "--no-first-run",
   "--no-default-browser-check",
   "--disable-background-networking",
   "--disable-background-timer-throttling",
   "--disable-backgrounding-occluded-windows",
   "--disable-component-update",
   "--disable-default-apps",
   "--disable-extensions",
   "--disable-renderer-backgrounding",
   "--disable-sync",
   "--disable-gpu-sandbox",
   "--enable-unsafe-webgpu",
   "--noerrdialogs",
   "--use-angle=metal",
   `http://127.0.0.1:${port}/c25-prepared-bench.html?samples=${samples}&frames=${frames}&dirty=${dirty ? 1 : 0}&flat=${flat ? 1 : 0}&threshold=${threshold}&guardrails=${guardrails ? 1 : 0}&raf=${raf ? 1 : 0}`,
];
const child = chromeArch
   ? spawn("arch", [`-${chromeArch}`, chrome, ...chromeArgs])
   : spawn(chrome, chromeArgs);
child.stderr.resume();
const timeout = setTimeout(() => rejectResult(new Error("C25 browser benchmark timed out")), 180000);
try {
   const raw = await result;
   if (raw.startsWith("error=")) {
      throw new Error(raw.slice("error=".length));
   }
   const metrics = Object.fromEntries(raw.split(";").map(field => {
      const separator = field.indexOf("=");
      const value = field.slice(separator + 1);
      return [
         field.slice(0, separator),
         value.includes(",") ? value.split(",").map(Number) : Number(value),
      ];
   }));
   const required = guardrails
      ? ["structural_hits", "structural_misses", "structural_bundle_creates", "structural_bundle_replays", "segmented_bundle_replays", "segmented_direct_draws", "tiny_misses", "clean_hits", "dirty_misses", "dynamic_commands_copied", "effect_commands_copied", "layer_commands_copied", "device_misses", "resource_update_hits", "resource_recreate_misses", "budget_cache_bytes", "budget_upload_bytes", "budget_buffer_grows"]
      : raf
      ? ["raf_frame_samples_ms", "raf_cpu_samples_ms"]
      : ["frame_p50_ms", "encode_p50_ms", "command_encode_p50_ms", "gpu_p50_ms"];
   for (const key of required) {
      const valid = raf
         ? Array.isArray(metrics[key]) && metrics[key].length === Number(frames)
         : Number.isFinite(metrics[key]);
      if (!valid) {
         throw new Error(`missing ${key}: ${raw}`);
      }
   }
   const json = `${JSON.stringify({ metrics }, null, 2)}\n`;
   if (output) {
      writeFileSync(output, json);
   } else {
      process.stdout.write(json);
   }
} finally {
   clearTimeout(timeout);
   child.kill("SIGTERM");
   server.close();
}
