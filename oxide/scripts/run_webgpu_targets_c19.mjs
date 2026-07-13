#!/usr/bin/env node

import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { readFileSync, statSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";

const root = process.argv[2];
const repeats = process.argv[3] || "50";
const prefix = process.argv[4] || "direct_first_ms";
const output = process.argv[5] || "";
if (!root) {
   throw new Error("usage: run_webgpu_targets_c19.mjs WEB_ROOT [REPEATS] [PREFIX] [OUTPUT]");
}

const benchHtml = `<!doctype html>
<html><body><canvas id="oxide-canvas" style="width:256px;height:256px"></canvas><script type="module">
import init, { OxideWebApp } from "./pkg/oxide_host_web.js";
const params = new URLSearchParams(window.location.search);
try {
   await init();
   let textureCreates = 0;
   let bindGroupCreates = 0;
   const createTexture = GPUDevice.prototype.createTexture;
   const createBindGroup = GPUDevice.prototype.createBindGroup;
   GPUDevice.prototype.createTexture = function(...args) {
      textureCreates += 1;
      return createTexture.apply(this, args);
   };
   GPUDevice.prototype.createBindGroup = function(...args) {
      bindGroupCreates += 1;
      return createBindGroup.apply(this, args);
   };
   const app = await OxideWebApp.newAsync("oxide-canvas");
   const constructionTextureCreates = textureCreates;
   const constructionBindGroupCreates = bindGroupCreates;
   GPUDevice.prototype.createTexture = createTexture;
   GPUDevice.prototype.createBindGroup = createBindGroup;
   const canvas = document.getElementById("oxide-canvas");
   const warmupMetrics = await app.bench_webgpu_targets_c19(1);
   const metrics = await app.bench_webgpu_targets_c19(Number(params.get("repeats")));
   await fetch("/__c19_result", {
      method: "POST",
      body: JSON.stringify({
         metrics,
         warmup_metrics: warmupMetrics,
         construction_texture_creates: constructionTextureCreates,
         construction_bind_group_creates: constructionBindGroupCreates,
         canvas_width: canvas.width,
         canvas_height: canvas.height,
         css_width: canvas.clientWidth,
         css_height: canvas.clientHeight,
         dpr: devicePixelRatio,
      }),
   });
} catch (error) {
   await fetch("/__c19_result", { method: "POST", body: JSON.stringify({ error: error?.stack || String(error) }) });
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
   if (request.method === "POST" && request.url === "/__c19_result") {
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
   const requestPath = request.url.split("?", 1)[0];
   if (requestPath === "/c19-target-bench.html" || requestPath === "/") {
      response.writeHead(200, {
         "Content-Type": "text/html",
         "Cross-Origin-Opener-Policy": "same-origin",
         "Cross-Origin-Embedder-Policy": "require-corp",
      });
      response.end(benchHtml);
      return;
   }
   if (requestPath.includes("..")) {
      response.writeHead(400);
      response.end();
      return;
   }
   try {
      const path = join(root, requestPath);
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
const chromeArch = process.env.CHROME_ARCH || "arm64";
const chromeArgs = [
   "--headless=new",
   "--no-first-run",
   "--no-default-browser-check",
   "--disable-background-networking",
   "--disable-component-update",
   "--disable-default-apps",
   "--disable-extensions",
   "--disable-sync",
   "--disable-gpu-sandbox",
   "--enable-unsafe-webgpu",
   "--force-device-scale-factor=2",
   "--noerrdialogs",
   "--use-angle=metal",
   `http://127.0.0.1:${port}/c19-target-bench.html?repeats=${repeats}`,
];
const child = chromeArch
   ? spawn("arch", [`-${chromeArch}`, chrome, ...chromeArgs])
   : spawn(chrome, chromeArgs);
child.stderr.resume();
const timeout = setTimeout(() => rejectResult(new Error("C19 browser benchmark timed out")), 180000);
try {
   const page = JSON.parse(await result);
   if (page.error) {
      throw new Error(page.error);
   }
   const parseMetrics = raw => Object.fromEntries(raw.split(";").map(field => {
      const separator = field.indexOf("=");
      const key = field.slice(0, separator);
      const value = field.slice(separator + 1);
      return [key, value.includes(",") ? value.split(",").map(Number) : Number(value)];
   }));
   const metrics = parseMetrics(page.metrics);
   const warmupMetrics = parseMetrics(page.warmup_metrics);
   Object.assign(metrics, page);
   delete metrics.metrics;
   delete metrics.warmup_metrics;
   metrics.warmup_metrics = warmupMetrics;
   const samples = metrics[prefix];
   const warmupValue = warmupMetrics[prefix];
   const warmups = Array.isArray(warmupValue) ? warmupValue : [warmupValue];
   if (!Array.isArray(samples) || samples.length !== Number(repeats) || samples.some(value => !Number.isFinite(value))) {
      throw new Error(`missing ${prefix} C19 samples: ${page.metrics}`);
   }
   if (!Array.isArray(warmups) || warmups.length !== 1 || warmups.some(value => !Number.isFinite(value))) {
      throw new Error(`missing ${prefix} C19 warmup: ${page.warmup_metrics}`);
   }
   const evidence = { warmups, samples, metrics };
   const json = `${JSON.stringify(evidence, null, 2)}\n`;
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
