#!/usr/bin/env node

import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { readFileSync, statSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";

const root = process.argv[2];
const samples = process.argv[3] || "3";
const frames = process.argv[4] || "24";
const prefix = process.argv[5] || "glyphs";
const output = process.argv[6] || "";
if (!root) {
   throw new Error("usage: run_webgpu_geometry_c16.mjs WEB_ROOT [SAMPLES] [FRAMES] [PREFIX] [OUTPUT]");
}

const benchHtml = `<!doctype html>
<html><body><canvas id="oxide-canvas"></canvas><script type="module">
import init, { OxideWebApp } from "./pkg/oxide_host_web.js";
const params = new URLSearchParams(window.location.search);
try {
   await init();
   const app = await OxideWebApp.newAsync("oxide-canvas");
   app.prewarm_webgpu_bench_resources();
   const result = await app.bench_webgpu_geometry_c16(Number(params.get("samples")), Number(params.get("frames")));
   await fetch("/__c16_result", { method: "POST", body: result });
} catch (error) {
   await fetch("/__c16_result", { method: "POST", body: "error=" + (error?.stack || error) });
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
   if (request.method === "POST" && request.url === "/__c16_result") {
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
   if (request.url.split("?", 1)[0] === "/c16-geometry-bench.html") {
      response.writeHead(200, {
         "Content-Type": "text/html",
         "Cross-Origin-Opener-Policy": "same-origin",
         "Cross-Origin-Embedder-Policy": "require-corp",
      });
      response.end(benchHtml);
      return;
   }
   const relative = request.url.split("?", 1)[0] === "/" ? "/c16-geometry-bench.html" : request.url.split("?", 1)[0];
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
   "--disable-component-update",
   "--disable-default-apps",
   "--disable-extensions",
   "--disable-sync",
   "--disable-gpu-sandbox",
   "--enable-unsafe-webgpu",
   "--noerrdialogs",
   "--use-angle=metal",
   `http://127.0.0.1:${port}/c16-geometry-bench.html?samples=${samples}&frames=${frames}`,
];
const child = chromeArch
   ? spawn("arch", [`-${chromeArch}`, chrome, ...chromeArgs])
   : spawn(chrome, chromeArgs);
child.stderr.resume();
const timeout = setTimeout(() => rejectResult(new Error("C16 browser benchmark timed out")), 120000);
try {
   const raw = await result;
   if (raw.startsWith("error=")) {
      throw new Error(raw.slice("error=".length));
   }
   const metrics = Object.fromEntries(raw.split(";").map(field => {
      const separator = field.indexOf("=");
      const key = field.slice(0, separator);
      const value = Number(field.slice(separator + 1));
      return [key, value];
   }));
   const sample = metrics[`${prefix}_p50_ms`];
   const warmup = metrics[`${prefix}_warmup_ms`];
   if (!Number.isFinite(sample) || !Number.isFinite(warmup)) {
      throw new Error(`missing ${prefix} benchmark metrics: ${raw}`);
   }
   const evidence = { warmups: [warmup], samples: [sample], metrics };
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
