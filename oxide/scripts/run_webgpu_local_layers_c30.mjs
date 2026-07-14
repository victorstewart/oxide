#!/usr/bin/env node

import { createServer } from "node:http";
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { readFileSync, statSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";

const root = process.argv[2];
const width = process.argv[3] || "1920";
const height = process.argv[4] || "1080";
const samples = process.argv[5] || "1";
const frames = process.argv[6] || "150";
const oneDirty = process.argv[7] === "1";
const output = process.argv[8] || "";
const guardrails = process.argv[9] === "1";
if (!root)
{
   throw new Error("usage: run_webgpu_local_layers_c30.mjs WEB_ROOT [WIDTH] [HEIGHT] [SAMPLES] [FRAMES] [ONE_DIRTY] [OUTPUT]");
}

const kernelFileCount = () => Number(execFileSync("/usr/sbin/sysctl", ["-n", "kern.num_files"], { encoding: "utf8" }).trim());
const benchmarkChromePresent = () => spawnSync("/usr/bin/pgrep", ["-f", "[c]30-local-layers.html"]).status === 0;
const filesBefore = kernelFileCount();
if (benchmarkChromePresent())
{
   throw new Error("a prior C30 Chrome process is still running");
}
if (!Number.isFinite(filesBefore) || filesBefore > 100_000)
{
   throw new Error(`unsafe pre-run kern.num_files=${filesBefore}`);
}

const benchHtml = `<!doctype html>
<html><body><canvas id="oxide-canvas"></canvas><script type="module">
import init, { OxideWebApp } from "./pkg/oxide_host_web.js";
const params = new URLSearchParams(window.location.search);
try {
   await init();
   const app = await OxideWebApp.newAsync("oxide-canvas");
   app.prewarm_webgpu_bench_resources();
   const result = params.get("guardrails") === "1"
      ? await app.bench_webgpu_local_layer_guardrails_c30()
      : await app.bench_webgpu_local_layers_c30(
         Number(params.get("width")),
         Number(params.get("height")),
         Number(params.get("samples")),
         Number(params.get("frames")),
         params.get("one_dirty") === "1",
      );
   await fetch("/__c30_result", { method: "POST", body: result });
} catch (error) {
   await fetch("/__c30_result", { method: "POST", body: "error=" + (error?.stack || error) });
}
</script></body></html>`;

const mime = path => extname(path) === ".wasm"
   ? "application/wasm"
   : extname(path) === ".js"
   ? "text/javascript"
   : "text/html";
let resolveResult;
let rejectResult;
const result = new Promise((resolve, reject) => {
   resolveResult = resolve;
   rejectResult = reject;
});
const server = createServer((request, response) => {
   if (request.method === "POST" && request.url === "/__c30_result")
   {
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
   if (request.url.split("?", 1)[0] === "/c30-local-layers.html")
   {
      response.writeHead(200, {
         "Content-Type": "text/html",
         "Cross-Origin-Opener-Policy": "same-origin",
         "Cross-Origin-Embedder-Policy": "require-corp",
      });
      response.end(benchHtml);
      return;
   }
   const relative = request.url.split("?", 1)[0] === "/"
      ? "/c30-local-layers.html"
      : request.url.split("?", 1)[0];
   if (relative.includes(".."))
   {
      response.writeHead(400);
      response.end();
      return;
   }
   try
   {
      const path = join(root, relative);
      if (!statSync(path).isFile())
      {
         throw new Error("not a file");
      }
      response.writeHead(200, {
         "Content-Type": mime(path),
         "Cross-Origin-Opener-Policy": "same-origin",
         "Cross-Origin-Embedder-Policy": "require-corp",
      });
      response.end(readFileSync(path));
   }
   catch (_error)
   {
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
   `http://127.0.0.1:${port}/c30-local-layers.html?width=${width}&height=${height}&samples=${samples}&frames=${frames}&one_dirty=${oneDirty ? 1 : 0}&guardrails=${guardrails ? 1 : 0}`,
];
const child = chromeArch
   ? spawn("arch", [`-${chromeArch}`, chrome, ...chromeArgs])
   : spawn(chrome, chromeArgs);
child.stderr.resume();
const timeout = setTimeout(() => rejectResult(new Error("C30 browser benchmark timed out")), 240_000);
let raw;
try
{
   raw = await result;
}
finally
{
   clearTimeout(timeout);
   if (child.exitCode === null)
   {
      child.kill("SIGTERM");
      await new Promise(resolve => {
         const force = setTimeout(() => {
            child.kill("SIGKILL");
            resolve();
         }, 5_000);
         child.once("exit", () => {
            clearTimeout(force);
            resolve();
         });
      });
   }
   await new Promise(resolve => server.close(resolve));
}

const filesAfter = kernelFileCount();
if (benchmarkChromePresent())
{
   throw new Error("the C30 Chrome process did not terminate");
}
if (!Number.isFinite(filesAfter) || filesAfter > 100_000 || filesAfter > filesBefore + 10_000)
{
   throw new Error(`unsafe post-run kern.num_files=${filesAfter}, before=${filesBefore}`);
}
if (raw.startsWith("error="))
{
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
   ? [
      "clean_hits",
      "dirty_misses",
      "resize_hits",
      "scale_misses",
      "purge_misses",
      "device_misses",
      "resource_clean_hits",
      "resource_update_misses",
      "resource_recreate_misses",
      "edge_hits",
   ]
   : [
      "cpu_p50_ms",
      "gpu_p50_ms",
      "gpu_sample_count",
      "layer_texture_bytes",
      "layer_target_pixels",
      "layer_cache_hits_avg",
      "layer_cache_misses_avg",
      "render_passes_avg",
   ];
for (const key of required)
{
   if (!Number.isFinite(metrics[key]))
   {
      throw new Error(`missing ${key}: ${raw}`);
   }
}
if (!guardrails && (!Array.isArray(metrics.gpu_samples_ms)
   || metrics.gpu_samples_ms.length !== Number(metrics.gpu_sample_count)))
{
   throw new Error(`invalid C30 GPU sample population: ${raw}`);
}
const json = `${JSON.stringify({ metrics, health: { kern_num_files_before: filesBefore, kern_num_files_after: filesAfter } }, null, 2)}\n`;
if (output)
{
   writeFileSync(output, json);
}
else
{
   process.stdout.write(json);
}
