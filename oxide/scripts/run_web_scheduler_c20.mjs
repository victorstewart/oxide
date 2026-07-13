#!/usr/bin/env node

import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { readFileSync, statSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";

const root = process.argv[2];
const metricPath = process.argv[3] || "resize.submitted_frames";
const output = process.argv[4] || "";
if (!root) {
   throw new Error("usage: run_web_scheduler_c20.mjs WEB_ROOT [METRIC_PATH] [OUTPUT]");
}

const benchHtml = `<!doctype html>
<html><head><style>
html,body { width:100%; height:100%; margin:0; overflow:hidden; }
#oxide-canvas { display:block; width:100vw; height:100vh; touch-action:none; }
</style></head><body><canvas id="oxide-canvas"></canvas><script type="module">
import init, { OxideWebApp } from "./pkg/oxide_host_web.js";
const nextFrame = () => new Promise(resolve => requestAnimationFrame(resolve));
const parse = raw => Object.fromEntries(String(raw).split(";").filter(Boolean).map(field => {
   const split = field.indexOf("=");
   return [field.slice(0, split), Number(field.slice(split + 1))];
}));
const waitForIdle = async app => {
   let previousSubmissions = -1;
   let stableFrames = 0;
   for (let frame = 0; frame < 24; frame += 1) {
      await nextFrame();
      const stats = parse(app.web_scheduler_metrics());
      if (stats.raf_pending === 0 && stats.submitted_frames === previousSubmissions) {
         stableFrames += 1;
         if (stableFrames >= 2) {
            return stats;
         }
      } else {
         stableFrames = 0;
      }
      previousSubmissions = stats.submitted_frames;
   }
   throw new Error("web scheduler did not become idle");
};
const latencyDistribution = samples => {
   const sorted = [...samples].sort((a, b) => a - b);
   const percentile = fraction => {
      const position = (sorted.length - 1) * fraction;
      const lower = Math.floor(position);
      const upper = Math.ceil(position);
      const weight = position - lower;
      return sorted[lower] * (1 - weight) + sorted[upper] * weight;
   };
   return {
      samples_ms: samples,
      p50_ms: percentile(0.50),
      p95_ms: percentile(0.95),
      p99_ms: percentile(0.99),
      peak_ms: sorted[sorted.length - 1],
   };
};
const runScenario = async (app, eventCount, action) => {
   await waitForIdle(app);
   app.reset_web_scheduler_metrics();
   const start = performance.now();
   const event = await action();
   const eventStart = Number.isFinite(event?.timestamp_ms) ? event.timestamp_ms : start;
   const submissionsBefore = Number.isFinite(event?.submissions_before)
      ? event.submissions_before
      : 0;
   let stats = parse(app.web_scheduler_metrics());
   let eventToVisibleMs = stats.submitted_frames > submissionsBefore
      ? performance.now() - eventStart
      : 0;
   while (stats.submitted_frames <= submissionsBefore) {
      await nextFrame();
      stats = parse(app.web_scheduler_metrics());
      if (stats.submitted_frames > submissionsBefore) {
         eventToVisibleMs = performance.now() - eventStart;
      }
   }
   stats = await waitForIdle(app);
   const eventToVisible = Array.isArray(event?.event_to_visible_samples_ms)
      ? latencyDistribution(event.event_to_visible_samples_ms)
      : latencyDistribution([eventToVisibleMs]);
   const rafTimestamps = Array.isArray(event?.raf_timestamps_ms) ? event.raf_timestamps_ms : [];
   const rafDeltas = rafTimestamps.slice(1).map((value, index) => value - rafTimestamps[index]);
   const sortedDeltas = [...rafDeltas].sort((a, b) => a - b);
   const frameInterval = sortedDeltas.length === 0
      ? 0
      : sortedDeltas[Math.floor(sortedDeltas.length / 2)];
   const missedFrames = frameInterval <= 0
      ? 0
      : rafDeltas.reduce((total, delta) => total + Math.max(0, Math.round(delta / frameInterval) - 1), 0);
   return {
      ...stats,
      events: eventCount,
      submissions_per_event: stats.submitted_frames / eventCount,
      raf_callbacks_per_event: stats.raf_callbacks / eventCount,
      event_to_visible_ms: eventToVisible.p99_ms,
      event_to_visible: eventToVisible,
      raf_deltas_ms: rafDeltas,
      missed_frames: missedFrames,
   };
};
const repeatedInput = async (app, repetitions, action) => {
   const eventToVisibleSamplesMs = [];
   for (let sample = 0; sample < repetitions; sample += 1) {
      await waitForIdle(app);
      const submissionsBefore = parse(app.web_scheduler_metrics()).submitted_frames;
      const timestampMs = performance.now();
      action();
      let stats = parse(app.web_scheduler_metrics());
      while (stats.submitted_frames <= submissionsBefore) {
         await nextFrame();
         stats = parse(app.web_scheduler_metrics());
      }
      eventToVisibleSamplesMs.push(performance.now() - timestampMs);
   }
   return { event_to_visible_samples_ms: eventToVisibleSamplesMs };
};
try {
   const discreteSampleCount = 100;
   const pointerSampleCount = 240;
   await init();
   const app = await OxideWebApp.newAsync("oxide-canvas");
   const canvas = document.getElementById("oxide-canvas");
   app.set_scene(1);
   app.start();
   await waitForIdle(app);

   app.reset_web_scheduler_metrics();
   for (let frame = 0; frame < 8; frame += 1) {
      await nextFrame();
   }
   const idle = parse(app.web_scheduler_metrics());

   const click = await runScenario(app, discreteSampleCount * 2, () => repeatedInput(app, discreteSampleCount, () => {
      canvas.dispatchEvent(new PointerEvent("pointerdown", {
         bubbles: true, cancelable: true, clientX: 80, clientY: 72, buttons: 1,
      }));
      canvas.dispatchEvent(new PointerEvent("pointerup", {
         bubbles: true, cancelable: true, clientX: 80, clientY: 72, buttons: 0,
      }));
   }));
   app.set_scene(4);
   await waitForIdle(app);
   const key = await runScenario(app, discreteSampleCount * 2, () => repeatedInput(app, discreteSampleCount, () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
      window.dispatchEvent(new KeyboardEvent("keyup", { key: "ArrowRight", bubbles: true }));
   }));
   app.set_scene(1);
   await waitForIdle(app);
   const pointer240hz = await runScenario(app, pointerSampleCount, () => new Promise(resolve => {
      let sample = 0;
      let trackRaf = true;
      const rafTimestampsMs = [];
      const eventToVisibleSamplesMs = [];
      const visibleSamples = [];
      const trackFrame = timestampMs => {
         if (!trackRaf) {
            return;
         }
         rafTimestampsMs.push(timestampMs);
         requestAnimationFrame(trackFrame);
      };
      requestAnimationFrame(trackFrame);
      const dispatch = () => {
         const timestampMs = performance.now();
         canvas.dispatchEvent(new PointerEvent("pointermove", {
            bubbles: true,
            cancelable: true,
            clientX: 24 + sample % 240,
            clientY: 96,
            movementX: 1,
            movementY: 0,
            buttons: 1,
         }));
         visibleSamples.push(new Promise(resolveVisible => requestAnimationFrame(() => {
            eventToVisibleSamplesMs.push(performance.now() - timestampMs);
            resolveVisible();
         })));
         sample += 1;
         if (sample === pointerSampleCount) {
            trackRaf = false;
            Promise.all(visibleSamples).then(() => resolve({
               event_to_visible_samples_ms: eventToVisibleSamplesMs,
               raf_timestamps_ms: rafTimestampsMs,
            }));
         } else {
            setTimeout(dispatch, 1000 / 240);
         }
      };
      dispatch();
   }));
   const resize = await runScenario(app, 100, () => {
      for (let event = 0; event < 100; event += 1) {
         window.dispatchEvent(new Event("resize"));
      }
   });
   const redraw = await runScenario(app, 100, () => {
      for (let event = 0; event < 100; event += 1) {
         window.dispatchEvent(new Event("oxide-redraw"));
      }
   });
   const style = await runScenario(app, 2, () => {
      canvas.style.width = "calc(100vw - 32px)";
      window.dispatchEvent(new Event("oxide-redraw"));
   });
   const expectedCssWidth = Math.max(1, window.innerWidth - 32);
   const styleWidthMatches =
      Math.abs(style.css_width - expectedCssWidth) <= 1
      && style.physical_width === Math.round(style.css_width * style.scale);

   const result = { idle, click, key, pointer240hz, resize, redraw, style, style_width_matches: styleWidthMatches };
   await fetch("/__c20_result", { method: "POST", body: JSON.stringify(result) });
} catch (error) {
   await fetch("/__c20_result", {
      method: "POST",
      body: JSON.stringify({ error: error?.stack || String(error) }),
   });
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
   if (request.method === "POST" && request.url === "/__c20_result") {
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
   if (requestPath === "/" || requestPath === "/c20-scheduler-bench.html") {
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
   `http://127.0.0.1:${port}/c20-scheduler-bench.html`,
];
const child = chromeArch
   ? spawn("arch", [`-${chromeArch}`, chrome, ...chromeArgs])
   : spawn(chrome, chromeArgs);
child.stderr.resume();
const timeout = setTimeout(() => rejectResult(new Error("C20 browser benchmark timed out")), 180000);
try {
   const metrics = JSON.parse(await result);
   if (metrics.error) {
      throw new Error(metrics.error);
   }
   const sample = metricPath.split(".").reduce((value, key) => value?.[key], metrics);
   if (!Number.isFinite(sample)) {
      throw new Error(`missing C20 metric ${metricPath}`);
   }
   const evidence = { warmups: [sample], samples: [sample], metrics };
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
