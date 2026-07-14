#!/usr/bin/env node

import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..");
const webRoot = join(repoRoot, "host", "web-app", "www");
const defaultAppGolden = join(repoRoot, "goldens", "snapshots", "webgpu_browser.png");
const defaultIdMaskGolden = join(repoRoot, "goldens", "snapshots", "webgpu_id_mask_compositor.png");
const defaultScene3dGolden = join(repoRoot, "goldens", "snapshots", "webgpu_scene3d.png");
const defaultGlyphGolden = join(repoRoot, "goldens", "snapshots", "webgpu_glyph_atlas.png");

function defaultGoldenForTarget(target)
{
   if (target === "app") {
      return defaultAppGolden;
   }
   if (target === "id-mask") {
      return defaultIdMaskGolden;
   }
   if (target === "scene3d") {
      return defaultScene3dGolden;
   }
   if (target === "glyph") {
      return defaultGlyphGolden;
   }
   throw new Error(`unknown capture target ${target}`);
}

function parseArgs(argv)
{
   let args = {
      chrome: process.env.CHROME_BIN || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      chromeArch: process.env.CHROME_ARCH || "",
      userDataDir: process.env.CHROME_USER_DATA_DIR || "",
      target: "app",
      golden: "",
      out: "",
      update: false,
      pixelTolerance: 15000,
      maxErrTolerance: 255,
      mseTolerance: 6500,
      virtualTimeBudget: 60000,
      captureRetries: 2,
      reportTimeoutMs: 90000,
      width: 320,
      height: 240,
      dpr: 1,
      jsonReport: "",
      markdownReport: "",
      rawReport: "",
      idMaskReferenceOut: "",
      idMaskMatrixOut: "",
      validateRawReport: "",
      selfTestMeasurement: false,
      reportOnly: false,
      traceJson: "",
      traceCategories: "gpu,viz,cc,blink,blink.user_timing,benchmark,disabled-by-default-gpu.service,disabled-by-default-devtools.timeline",
      traceDurationMs: 60000,
      reportDate: process.env.PERF_REPORT_DATE || new Date().toISOString().slice(0, 10),
      startupReport: "",
      startupRepeats: 0,
      canvasReport: "",
      canvasRepeats: 0,
      canvasSamples: 6,
      canvasFrames: 24,
      canvasQuads: 512,
      frameSamples: 8,
      framesPerSample: 30,
      rafFrames: 2000,
      rafResizeEvery: 0,
      rafScene: 0,
      idMaskSamples: 6,
      idMaskFrames: 24,
      uploadSamples: 6,
      uploadFrames: 24,
      scene3dSamples: 6,
      scene3dFrames: 24,
      mixedSamples: 6,
      mixedFrames: 24,
      architectureMatrix: false,
      rrectArchitectureOnly: false,
      idMaskCacheC33: false,
      idMaskCacheOnly: false,
      idMaskCacheRafOnly: false,
      idMaskCacheRafFrames: 160,
      preparedFlat: false,
   };

   for (let i = 0; i < argv.length; i++) {
      let arg = argv[i];
      let next = () => {
         i += 1;
         if (i >= argv.length) {
            throw new Error(`missing value for ${arg}`);
         }
         return argv[i];
      };
      if (arg === "--chrome") {
         args.chrome = next();
      } else if (arg === "--chrome-arch") {
         args.chromeArch = next();
      } else if (arg === "--user-data-dir") {
         args.userDataDir = next();
      } else if (arg === "--target" || arg === "--capture-target") {
         args.target = next();
      } else if (arg === "--golden") {
         args.golden = next();
      } else if (arg === "--out") {
         args.out = next();
      } else if (arg === "--update") {
         args.update = true;
      } else if (arg === "--pixel-tolerance") {
         args.pixelTolerance = Number(next());
      } else if (arg === "--max-err-tolerance") {
         args.maxErrTolerance = Number(next());
      } else if (arg === "--mse-tolerance") {
         args.mseTolerance = Number(next());
      } else if (arg === "--virtual-time-budget") {
         args.virtualTimeBudget = Number(next());
      } else if (arg === "--capture-retries") {
         args.captureRetries = Number(next());
      } else if (arg === "--report-timeout-ms") {
         args.reportTimeoutMs = Number(next());
      } else if (arg === "--width") {
         args.width = Number(next());
      } else if (arg === "--height") {
         args.height = Number(next());
      } else if (arg === "--dpr") {
         args.dpr = Number(next());
      } else if (arg === "--json-report") {
         args.jsonReport = next();
      } else if (arg === "--markdown-report") {
         args.markdownReport = next();
      } else if (arg === "--raw-report") {
         args.rawReport = next();
      } else if (arg === "--id-mask-reference-out") {
         args.idMaskReferenceOut = next();
      } else if (arg === "--id-mask-matrix-out") {
         args.idMaskMatrixOut = next();
      } else if (arg === "--validate-raw-report") {
         args.validateRawReport = next();
      } else if (arg === "--self-test-measurement") {
         args.selfTestMeasurement = true;
      } else if (arg === "--report-only") {
         args.reportOnly = true;
      } else if (arg === "--trace-json") {
         args.traceJson = next();
      } else if (arg === "--trace-categories") {
         args.traceCategories = next();
      } else if (arg === "--trace-duration-ms") {
         args.traceDurationMs = Number(next());
      } else if (arg === "--report-date") {
         args.reportDate = next();
      } else if (arg === "--startup-report") {
         args.startupReport = next();
      } else if (arg === "--startup-repeats") {
         args.startupRepeats = Number(next());
      } else if (arg === "--canvas-report") {
         args.canvasReport = next();
      } else if (arg === "--canvas-repeats") {
         args.canvasRepeats = Number(next());
      } else if (arg === "--canvas-samples") {
         args.canvasSamples = Number(next());
      } else if (arg === "--canvas-frames") {
         args.canvasFrames = Number(next());
      } else if (arg === "--canvas-quads") {
         args.canvasQuads = Number(next());
      } else if (arg === "--frame-samples") {
         args.frameSamples = Number(next());
      } else if (arg === "--frames-per-sample") {
         args.framesPerSample = Number(next());
      } else if (arg === "--raf-frames") {
         args.rafFrames = Number(next());
      } else if (arg === "--raf-resize-every") {
         args.rafResizeEvery = Number(next());
      } else if (arg === "--raf-scene") {
         args.rafScene = Number(next());
      } else if (arg === "--id-mask-samples") {
         args.idMaskSamples = Number(next());
      } else if (arg === "--id-mask-frames") {
         args.idMaskFrames = Number(next());
      } else if (arg === "--upload-samples") {
         args.uploadSamples = Number(next());
      } else if (arg === "--upload-frames") {
         args.uploadFrames = Number(next());
      } else if (arg === "--scene3d-samples") {
         args.scene3dSamples = Number(next());
      } else if (arg === "--scene3d-frames") {
         args.scene3dFrames = Number(next());
      } else if (arg === "--mixed-samples") {
         args.mixedSamples = Number(next());
      } else if (arg === "--mixed-frames") {
         args.mixedFrames = Number(next());
      } else if (arg === "--architecture-matrix") {
         args.architectureMatrix = true;
      } else if (arg === "--rrect-architecture-only") {
         args.rrectArchitectureOnly = true;
      } else if (arg === "--id-mask-cache-c33") {
         args.idMaskCacheC33 = true;
      } else if (arg === "--id-mask-cache-only") {
         args.idMaskCacheOnly = true;
      } else if (arg === "--id-mask-cache-raf-only") {
         args.idMaskCacheRafOnly = true;
      } else if (arg === "--id-mask-cache-raf-frames") {
         args.idMaskCacheRafFrames = Number(next());
      } else if (arg === "--prepared-flat") {
         args.preparedFlat = true;
      } else {
         throw new Error(`unknown argument ${arg}`);
      }
   }

   if (args.target !== "app" && args.target !== "glyph" && args.target !== "id-mask" && args.target !== "scene3d" && args.target !== "prepared" && args.target !== "local-layers" && args.target !== "rrect") {
      throw new Error("--target must be app, glyph, id-mask, scene3d, prepared, local-layers, or rrect");
   }
   if (!args.golden) {
      args.golden = defaultGoldenForTarget(args.target);
   }
   if (!Number.isFinite(args.width) || args.width <= 0 || !Number.isFinite(args.height) || args.height <= 0) {
      throw new Error("width and height must be positive numbers");
   }
   if (![1, 2, 3].includes(args.dpr)) {
      throw new Error("dpr must be 1, 2, or 3");
   }
   if (!Number.isInteger(args.rafResizeEvery) || args.rafResizeEvery < 0) {
      throw new Error("raf resize interval must be a nonnegative integer");
   }
   if (!Number.isInteger(args.rafScene) || args.rafScene < 0 || args.rafScene > 16) {
      throw new Error("raf scene must be an integer from 0 through 16");
   }
   if (!Number.isFinite(args.reportTimeoutMs) || args.reportTimeoutMs <= 0) {
      throw new Error("report timeout must be a positive number");
   }
   if (!Number.isFinite(args.captureRetries) || args.captureRetries < 0) {
      throw new Error("capture retries must be a non-negative number");
   }
   if ((args.jsonReport || args.markdownReport) && !args.traceJson && !args.validateRawReport) {
      throw new Error("--trace-json is required when writing browser WebGPU reports");
   }
   if (args.traceJson && !args.traceCategories) {
      throw new Error("trace categories must be non-empty when --trace-json is set");
   }
   if (!Number.isFinite(args.traceDurationMs) || args.traceDurationMs <= 0) {
      throw new Error("trace duration must be a positive number");
   }
   if (args.startupReport) {
      if (args.jsonReport || args.markdownReport || args.traceJson) {
         throw new Error("--startup-report cannot be combined with --json-report, --markdown-report, or --trace-json");
      }
      if (args.startupRepeats === 0) {
         args.startupRepeats = 5;
      }
   } else if (args.startupRepeats !== 0) {
      throw new Error("--startup-repeats requires --startup-report");
   }
   if (args.startupRepeats !== 0) {
      if (!Number.isFinite(args.startupRepeats) || args.startupRepeats <= 0) {
         throw new Error("startup repeats must be a positive number");
      }
      args.startupRepeats = Math.trunc(args.startupRepeats);
   }
   if (args.canvasReport) {
      if (args.jsonReport || args.markdownReport || args.traceJson || args.startupReport) {
         throw new Error("--canvas-report cannot be combined with --json-report, --markdown-report, --trace-json, or --startup-report");
      }
      if (args.canvasRepeats === 0) {
         args.canvasRepeats = 5;
      }
   } else if (args.canvasRepeats !== 0) {
      throw new Error("--canvas-repeats requires --canvas-report");
   }
   if (args.canvasRepeats !== 0) {
      if (!Number.isFinite(args.canvasRepeats) || args.canvasRepeats <= 0) {
         throw new Error("canvas repeats must be a positive number");
      }
      args.canvasRepeats = Math.trunc(args.canvasRepeats);
   }
   args.reportTimeoutMs = Math.trunc(args.reportTimeoutMs);
   args.captureRetries = Math.trunc(args.captureRetries);
   args.traceDurationMs = Math.trunc(args.traceDurationMs);
   for (let key of ["frameSamples", "framesPerSample", "rafFrames", "idMaskSamples", "idMaskFrames", "idMaskCacheRafFrames", "uploadSamples", "uploadFrames", "scene3dSamples", "scene3dFrames", "mixedSamples", "mixedFrames", "canvasSamples", "canvasFrames", "canvasQuads"]) {
      if (!Number.isFinite(args[key]) || args[key] <= 0) {
         throw new Error(`${key} must be a positive number`);
      }
      args[key] = Math.trunc(args[key]);
   }
   if (args.rafFrames < 2000) {
      throw new Error("rafFrames must be at least 2000 for displayed-frame reports");
   }
   return args;
}

function mimeType(path)
{
   switch (extname(path)) {
      case ".html":
         return "text/html; charset=utf-8";
      case ".js":
         return "text/javascript; charset=utf-8";
      case ".wasm":
         return "application/wasm";
      case ".png":
         return "image/png";
      default:
         return "application/octet-stream";
   }
}

function startServer()
{
   let pendingReports = [];
   let nextReportPromise = () =>
      new Promise((resolvePromise, rejectPromise) => {
         pendingReports.push({ resolvePromise, rejectPromise });
      });

   let server = createServer((req, res) => {
      let url = new URL(req.url || "/", "http://127.0.0.1");
      if (req.method === "POST" && url.pathname === "/__oxide_report") {
         let body = "";
         let pending = pendingReports.shift();
         if (!pending) {
            res.writeHead(409);
            res.end("unexpected report");
            return;
         }
         req.on("data", chunk => {
            body += chunk.toString("utf8");
         });
         req.on("error", err => {
            pending.rejectPromise(err);
         });
         req.on("end", () => {
            try {
               pending.resolvePromise(JSON.parse(body));
               res.writeHead(204);
               res.end();
            } catch (err) {
               pending.rejectPromise(err);
               res.writeHead(400);
               res.end("bad report");
            }
         });
         return;
      }

      let rel = decodeURIComponent(url.pathname === "/" ? "/index.html" : url.pathname);
      if (rel.includes("..")) {
         res.writeHead(400);
         res.end("bad path");
         return;
      }
      let path = join(webRoot, rel);
      try {
         let st = statSync(path);
         if (!st.isFile()) {
            res.writeHead(404);
            res.end("not found");
            return;
         }
         res.writeHead(200, {
            "Content-Type": mimeType(path),
            "Cross-Origin-Opener-Policy": "same-origin",
            "Cross-Origin-Embedder-Policy": "require-corp",
         });
         res.end(readFileSync(path));
      } catch (_err) {
         res.writeHead(404);
         res.end("not found");
      }
   });

   return new Promise((resolvePromise, rejectPromise) => {
      server.once("error", rejectPromise);
      server.listen(0, "127.0.0.1", () => resolvePromise({ server, nextReportPromise }));
   });
}

function runChrome(args, url, out)
{
   return new Promise((resolvePromise, rejectPromise) => {
      let chromeArgs = [
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
         "--enable-precise-memory-info",
         "--js-flags=--expose-gc",
         "--metrics-recording-only",
         "--noerrdialogs",
         "--use-angle=metal",
         "--default-background-color=00000000",
         `--window-size=${args.width},${args.height}`,
         `--force-device-scale-factor=${args.dpr}`,
         `--screenshot=${out}`,
         url,
      ];
      if (args.virtualTimeBudget > 0) {
         chromeArgs.splice(7, 0, `--virtual-time-budget=${args.virtualTimeBudget}`);
      }
      if (args.userDataDir) {
         chromeArgs.splice(6, 0, `--user-data-dir=${args.userDataDir}`);
      }
      let { command, commandArgs } = chromeCommand(args, chromeArgs);
      let child = spawn(command, commandArgs, { stdio: "inherit" });

      child.once("error", err => {
         rejectPromise(err);
      });
      child.once("exit", code => {
         if (code === 0) {
            resolvePromise();
         } else {
            rejectPromise(new Error(`Chrome exited with status ${code}`));
         }
      });
   });
}

function addTraceChromeArgs(args, chromeArgs)
{
   if (!args.traceJson) {
      return;
   }
   mkdirSync(dirname(args.traceJson), { recursive: true });
   chromeArgs.splice(
      chromeArgs.length - 1,
      0,
      `--trace-startup=${args.traceCategories}`,
      `--trace-startup-file=${args.traceJson}`,
      `--trace-startup-duration=${Math.max(1, Math.ceil(args.traceDurationMs / 1000))}`,
      "--trace-startup-format=json",
   );
}

function runChromeForReport(args, url, reportPromise)
{
   return new Promise((resolvePromise, rejectPromise) => {
      let chromeArgs = [
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
         "--enable-precise-memory-info",
         "--js-flags=--expose-gc",
         "--metrics-recording-only",
         "--noerrdialogs",
         "--use-angle=metal",
         `--window-size=${args.width},${args.height}`,
         `--force-device-scale-factor=${args.dpr}`,
         url,
      ];
      if (args.userDataDir) {
         chromeArgs.splice(6, 0, `--user-data-dir=${args.userDataDir}`);
      }
      addTraceChromeArgs(args, chromeArgs);
      let { command, commandArgs } = chromeCommand(args, chromeArgs);
      let child = spawn(command, commandArgs, { stdio: "inherit" });
      let settled = false;
      let reportReady = false;
      let reportValue = null;
      let killTimer = null;
      let forceFinishTimer = null;

      let finish = (err, report) => {
         if (settled) {
            return;
         }
         settled = true;
         if (killTimer) {
            clearTimeout(killTimer);
         }
         if (forceFinishTimer) {
            clearTimeout(forceFinishTimer);
         }
         if (err) {
            rejectPromise(err);
         } else {
            resolvePromise(report);
         }
      };

      let shutdownAfterReport = report => {
         if (settled || reportReady) {
            return;
         }
         reportReady = true;
         reportValue = report;
         child.kill("SIGTERM");
         let graceMs = args.traceJson ? Math.max(15000, args.traceDurationMs + 5000) : 3000;
         killTimer = setTimeout(() => {
            if (child.exitCode === null && child.signalCode === null) {
               child.kill("SIGKILL");
            }
            if (!args.traceJson) {
               finish(null, reportValue);
            }
         }, graceMs);
         killTimer.unref();
         if (args.traceJson) {
            forceFinishTimer = setTimeout(() => {
               finish(null, reportValue);
            }, graceMs + 5000);
            forceFinishTimer.unref();
         }
      };

      child.once("error", err => {
         finish(err);
      });
      child.once("exit", code => {
         if (reportReady) {
            finish(null, reportValue);
         } else {
            finish(new Error(`Chrome exited before browser report with status ${code}`));
         }
      });
      waitForBrowserReport(reportPromise, args.reportTimeoutMs)
         .then(report => shutdownAfterReport(report))
         .catch(err => {
            child.kill("SIGTERM");
            finish(err);
         });
   });
}

function browserUrl(args, baseUrl, reportEndpoint, startupOnly = false, canvasDiag = false)
{
   let url = new URL(baseUrl);
   url.searchParams.set("frame_samples", String(args.frameSamples));
   url.searchParams.set("frames_per_sample", String(args.framesPerSample));
   url.searchParams.set("raf_frames", String(args.rafFrames));
   url.searchParams.set("raf_resize_every", String(args.rafResizeEvery));
   url.searchParams.set("raf_scene", String(args.rafScene));
   url.searchParams.set("id_mask_samples", String(args.idMaskSamples));
   url.searchParams.set("id_mask_frames", String(args.idMaskFrames));
   url.searchParams.set("upload_samples", String(args.uploadSamples));
   url.searchParams.set("upload_frames", String(args.uploadFrames));
   url.searchParams.set("scene3d_samples", String(args.scene3dSamples));
   url.searchParams.set("scene3d_frames", String(args.scene3dFrames));
   url.searchParams.set("mixed_samples", String(args.mixedSamples));
   url.searchParams.set("mixed_frames", String(args.mixedFrames));
   if (args.architectureMatrix) {
      url.searchParams.set("architecture_matrix", "1");
   }
   if (args.rrectArchitectureOnly) {
      url.searchParams.set("rrect_architecture_only", "1");
   }
   if (args.idMaskCacheC33) {
      url.searchParams.set("id_mask_cache_c33", "1");
   }
   if (args.idMaskCacheOnly) {
      url.searchParams.set("id_mask_cache_only", "1");
   }
   if (args.idMaskCacheRafOnly) {
      url.searchParams.set("id_mask_cache_raf_only", "1");
      url.searchParams.set("id_mask_cache_raf_frames", String(args.idMaskCacheRafFrames));
   }
   if (canvasDiag) {
      url.searchParams.set("canvas_diag", "1");
      url.searchParams.set("canvas_samples", String(args.canvasSamples));
      url.searchParams.set("canvas_frames", String(args.canvasFrames));
      url.searchParams.set("canvas_quads", String(args.canvasQuads));
   }
   url.searchParams.set("capture_target", args.target);
   if (args.preparedFlat) {
      url.searchParams.set("prepared_flat", "1");
   }
   url.searchParams.set("capture_width", String(args.width));
   url.searchParams.set("capture_height", String(args.height));
   if (!reportEndpoint) {
      url.searchParams.set("capture_only", "1");
   }
   if (reportEndpoint) {
      url.searchParams.set("report_endpoint", "1");
   }
   if (startupOnly) {
      url.searchParams.set("startup_only", "1");
   }
   return url.toString();
}

function persistedBrowserUrl(args)
{
   return `http://127.0.0.1:<ephemeral>/?frame_samples=${args.frameSamples}&frames_per_sample=${args.framesPerSample}&raf_frames=${args.rafFrames}&raf_resize_every=${args.rafResizeEvery}&raf_scene=${args.rafScene}&id_mask_samples=${args.idMaskSamples}&id_mask_frames=${args.idMaskFrames}&upload_samples=${args.uploadSamples}&upload_frames=${args.uploadFrames}&scene3d_samples=${args.scene3dSamples}&scene3d_frames=${args.scene3dFrames}&mixed_samples=${args.mixedSamples}&mixed_frames=${args.mixedFrames}&capture_target=${args.target}&capture_width=${args.width}&capture_height=${args.height}&report_endpoint=1`;
}

function persistedCanvasBrowserUrl(args)
{
   return `http://127.0.0.1:<ephemeral>/?canvas_diag=1&canvas_samples=${args.canvasSamples}&canvas_frames=${args.canvasFrames}&canvas_quads=${args.canvasQuads}&capture_target=${args.target}&capture_width=${args.width}&capture_height=${args.height}&report_endpoint=1`;
}

function chromeCommand(args, chromeArgs)
{
   if (!args.chromeArch) {
      return { command: args.chrome, commandArgs: chromeArgs };
   }
   return {
      command: "/usr/bin/arch",
      commandArgs: [`-${args.chromeArch}`, args.chrome, ...chromeArgs],
   };
}

function sleep(ms)
{
   return new Promise(resolvePromise => setTimeout(resolvePromise, ms));
}

const BENCHMARK_MARK_PREFIX = "oxide-webgpu-bench:";

function benchmarkLabelFromTraceName(name)
{
   if (!name.startsWith(BENCHMARK_MARK_PREFIX)) {
      return "";
   }
   let label = name.slice(BENCHMARK_MARK_PREFIX.length);
   for (let suffix of [":start", ":end"]) {
      if (label.endsWith(suffix)) {
         return label.slice(0, -suffix.length);
      }
   }
   return label;
}

function summarizeTraceEvents(path, events)
{
   let minTs = Number.POSITIVE_INFINITY;
   let maxTs = 0;
   let gpuEvents = 0;
   let webgpuEvents = 0;
   let angleEvents = 0;
   let rendererEvents = 0;
   let categories = new Set();
   let benchmarkTraceMarks = new Map();
   let benchmarkTraceStarts = new Map();
   let benchmarkTraceEnds = new Map();
   for (let event of events) {
      let cat = typeof event.cat === "string" ? event.cat : "";
      let name = typeof event.name === "string" ? event.name : "";
      let label = `${cat} ${name}`;
      let benchmarkLabel = benchmarkLabelFromTraceName(name);
      if (cat) {
         for (let item of cat.split(",")) {
            if (item) {
               categories.add(item);
            }
         }
      }
      if (event.ph !== "M" && Number.isFinite(event.ts)) {
         minTs = Math.min(minTs, event.ts);
         maxTs = Math.max(maxTs, event.ts);
      }
      if (/gpu|webgpu|dawn|angle|viz|compositor/i.test(label)) {
         gpuEvents += 1;
      }
      if (/webgpu|dawn/i.test(label)) {
         webgpuEvents += 1;
      }
      if (/angle/i.test(label)) {
         angleEvents += 1;
      }
      if (/renderer|render/i.test(label)) {
         rendererEvents += 1;
      }
      if (benchmarkLabel) {
         let entry = benchmarkTraceMarks.get(benchmarkLabel) || {
            id: benchmarkLabel,
            event_count: 0,
            duration_us: 0,
         };
         entry.event_count += 1;
         if (Number.isFinite(event.dur)) {
            entry.duration_us += event.dur;
         }
         benchmarkTraceMarks.set(benchmarkLabel, entry);
         if (Number.isFinite(event.ts) && event.ph === "b") {
            let starts = benchmarkTraceStarts.get(benchmarkLabel) || [];
            starts.push(event.ts);
            benchmarkTraceStarts.set(benchmarkLabel, starts);
         }
         if (Number.isFinite(event.ts) && event.ph === "e") {
            let ends = benchmarkTraceEnds.get(benchmarkLabel) || [];
            ends.push(event.ts);
            benchmarkTraceEnds.set(benchmarkLabel, ends);
         }
      }
   }
   let benchmarkMarks = [...benchmarkTraceMarks.values()].sort((a, b) => a.id.localeCompare(b.id));
   let benchmarkIntervals = traceBenchmarkIntervals(benchmarkTraceStarts, benchmarkTraceEnds, events);
   return {
      status: "collected",
      artifact: path,
      events: events.length,
      gpu_related_events: gpuEvents,
      webgpu_related_events: webgpuEvents,
      angle_related_events: angleEvents,
      renderer_related_events: rendererEvents,
      duration_us: Number.isFinite(minTs) ? Math.max(0, maxTs - minTs) : 0,
      category_count: categories.size,
      sample_categories: [...categories].sort().slice(0, 24),
      benchmark_trace_mark_status: benchmarkMarks.length > 0 ? "collected" : "not-collected",
      benchmark_trace_mark_count: benchmarkMarks.reduce((sum, entry) => sum + entry.event_count, 0),
      benchmark_trace_mark_labels: benchmarkMarks.map(entry => entry.id),
      benchmark_trace_marks: benchmarkMarks,
      benchmark_trace_interval_count: benchmarkIntervals.length,
      benchmark_trace_interval_labels: benchmarkIntervals.map(entry => entry.id),
      benchmark_trace_intervals: benchmarkIntervals,
   };
}

function traceBenchmarkIntervals(startsById, endsById, events)
{
   let intervals = [];
   for (let [id, starts] of startsById.entries()) {
      let sortedStarts = [...starts].sort((a, b) => a - b);
      let sortedEnds = [...(endsById.get(id) || [])].sort((a, b) => a - b);
      for (let startTs of sortedStarts) {
         let endIndex = sortedEnds.findIndex(endTs => endTs >= startTs);
         if (endIndex < 0) {
            continue;
         }
         let endTs = sortedEnds.splice(endIndex, 1)[0];
         intervals.push({
            id,
            start_ts: startTs,
            end_ts: endTs,
            duration_us: Math.max(0, endTs - startTs),
            event_count: 0,
            gpu_related_events: 0,
            webgpu_related_events: 0,
            angle_related_events: 0,
            renderer_related_events: 0,
            event_duration_us: 0,
         });
      }
   }
   intervals.sort((a, b) => a.start_ts - b.start_ts || a.id.localeCompare(b.id));
   for (let event of events) {
      if (event.ph === "M" || !Number.isFinite(event.ts)) {
         continue;
      }
      let cat = typeof event.cat === "string" ? event.cat : "";
      let name = typeof event.name === "string" ? event.name : "";
      let label = `${cat} ${name}`;
      for (let interval of intervals) {
         if (event.ts < interval.start_ts || event.ts > interval.end_ts) {
            continue;
         }
         interval.event_count += 1;
         if (/gpu|webgpu|dawn|angle|viz|compositor/i.test(label)) {
            interval.gpu_related_events += 1;
         }
         if (/webgpu|dawn/i.test(label)) {
            interval.webgpu_related_events += 1;
         }
         if (/angle/i.test(label)) {
            interval.angle_related_events += 1;
         }
         if (/renderer|render/i.test(label)) {
            interval.renderer_related_events += 1;
         }
         if (Number.isFinite(event.dur)) {
            interval.event_duration_us += event.dur;
         }
      }
   }
   return intervals;
}

function parseTraceJsonText(text, path)
{
   let trimmed = text.trim();
   try {
      return JSON.parse(trimmed);
   } catch (err) {
      let patched = trimmed.replace(/,\s*$/, "");
      try {
         return JSON.parse(`${patched}\n]}`);
      } catch (_patchedErr) {
         throw new Error(`parse Chrome trace ${path}: ${err.message}`);
      }
   }
}

async function loadTraceSummary(path, timeoutMs)
{
   if (!path) {
      return {
         status: "not-requested",
         artifact: "",
         events: 0,
         gpu_related_events: 0,
         webgpu_related_events: 0,
         angle_related_events: 0,
         renderer_related_events: 0,
         duration_us: 0,
         category_count: 0,
         sample_categories: [],
         benchmark_trace_mark_status: "not-requested",
         benchmark_trace_mark_count: 0,
         benchmark_trace_mark_labels: [],
         benchmark_trace_marks: [],
         benchmark_trace_interval_count: 0,
         benchmark_trace_interval_labels: [],
         benchmark_trace_intervals: [],
      };
   }
   let deadline = Date.now() + timeoutMs;
   let lastErr = null;
   while (Date.now() <= deadline) {
      try {
         let st = statSync(path);
         if (st.isFile() && st.size > 0) {
            let trace = parseTraceJsonText(readFileSync(path, "utf8"), path);
            let events = Array.isArray(trace) ? trace : trace.traceEvents;
            if (!Array.isArray(events)) {
               throw new Error("trace JSON does not contain traceEvents");
            }
            return summarizeTraceEvents(path, events);
         }
      } catch (err) {
         lastErr = err;
      }
      await sleep(100);
   }
   throw new Error(`failed to load Chrome trace ${path}: ${lastErr ? lastErr.message : "trace file not written"}`);
}

function readU32BE(bytes, offset)
{
   return bytes.readUInt32BE(offset);
}

function paeth(a, b, c)
{
   let p = a + b - c;
   let pa = Math.abs(p - a);
   let pb = Math.abs(p - b);
   let pc = Math.abs(p - c);
   if (pa <= pb && pa <= pc) {
      return a;
   }
   return pb <= pc ? b : c;
}

function loadPngRgba(path)
{
   let bytes = readFileSync(path);
   if (bytes.length < 8 || bytes.subarray(0, 8).compare(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) !== 0) {
      throw new Error(`${path} is not a PNG`);
   }

   let width = 0;
   let height = 0;
   let colorType = 0;
   let bitDepth = 0;
   let idat = [];
   for (let pos = 8; pos < bytes.length;) {
      let len = readU32BE(bytes, pos);
      let kind = bytes.subarray(pos + 4, pos + 8).toString("ascii");
      let chunk = bytes.subarray(pos + 8, pos + 8 + len);
      pos += 12 + len;
      if (kind === "IHDR") {
         width = readU32BE(chunk, 0);
         height = readU32BE(chunk, 4);
         bitDepth = chunk[8];
         colorType = chunk[9];
         if (bitDepth !== 8 || (colorType !== 2 && colorType !== 6) || chunk[10] !== 0 || chunk[11] !== 0 || chunk[12] !== 0) {
            throw new Error(`${path} uses unsupported PNG encoding`);
         }
      } else if (kind === "IDAT") {
         idat.push(chunk);
      } else if (kind === "IEND") {
         break;
      }
   }

   let channels = colorType === 6 ? 4 : 3;
   let stride = width * channels;
   let source = inflateSync(Buffer.concat(idat));
   let rgba = Buffer.alloc(width * height * 4);
   let prev = Buffer.alloc(stride);
   let srcOffset = 0;
   let dstOffset = 0;
   for (let y = 0; y < height; y++) {
      let filter = source[srcOffset++];
      let row = Buffer.alloc(stride);
      for (let x = 0; x < stride; x++) {
         let left = x >= channels ? row[x - channels] : 0;
         let up = prev[x];
         let upLeft = x >= channels ? prev[x - channels] : 0;
         let value = source[srcOffset++];
         if (filter === 0) {
            row[x] = value;
         } else if (filter === 1) {
            row[x] = (value + left) & 255;
         } else if (filter === 2) {
            row[x] = (value + up) & 255;
         } else if (filter === 3) {
            row[x] = (value + Math.floor((left + up) / 2)) & 255;
         } else if (filter === 4) {
            row[x] = (value + paeth(left, up, upLeft)) & 255;
         } else {
            throw new Error(`${path} has unsupported PNG filter ${filter}`);
         }
      }

      for (let x = 0; x < width; x++) {
         let src = x * channels;
         rgba[dstOffset++] = row[src];
         rgba[dstOffset++] = row[src + 1];
         rgba[dstOffset++] = row[src + 2];
         rgba[dstOffset++] = channels === 4 ? row[src + 3] : 255;
      }
      prev = row;
   }

   return { width, height, rgba };
}

function comparePngs(capture, golden)
{
   if (capture.width !== golden.width || capture.height !== golden.height) {
      throw new Error(`golden size mismatch: capture ${capture.width}x${capture.height}, golden ${golden.width}x${golden.height}`);
   }

   let pixdiff = 0;
   let maxErr = 0;
   let sumSq = 0;
   for (let i = 0; i < capture.rgba.length; i += 4) {
      let changed = false;
      for (let c = 0; c < 4; c++) {
         let d = Math.abs(capture.rgba[i + c] - golden.rgba[i + c]);
         maxErr = Math.max(maxErr, d);
         sumSq += d * d;
         changed = changed || d !== 0;
      }
      if (changed) {
         pixdiff += 1;
      }
   }

   return { pixdiff, maxErr, mse: sumSq / capture.rgba.length };
}

function assertRendered(image, target)
{
   if (target === "id-mask") {
      assertIdMaskRendered(image);
   } else if (target === "scene3d") {
      assertScene3dRendered(image);
   } else if (target === "glyph") {
      assertGlyphRendered(image);
   } else if (target === "prepared") {
      assertPreparedRendered(image);
   } else if (target === "local-layers") {
      assertLocalLayersRendered(image);
   } else if (target === "rrect") {
      assertRRectRendered(image);
   } else {
      assertAppRendered(image);
   }
}

function assertRRectRendered(image)
{
   let blue = 0;
   let dark = 0;
   for (let i = 0; i < image.rgba.length; i += 4) {
      let r = image.rgba[i];
      let g = image.rgba[i + 1];
      let b = image.rgba[i + 2];
      if (b > 140 && g > 80 && b > r * 1.8) {
         blue += 1;
      }
      if (r < 16 && g < 20 && b < 32) {
         dark += 1;
      }
   }
   if (blue < 12000 || dark < 20000) {
      throw new Error(`capture does not look like the RRect scene: blue=${blue} dark=${dark}`);
   }
}

function assertLocalLayersRendered(image)
{
   let dark = 0;
   let colorful = 0;
   let bright = 0;
   for (let i = 0; i < image.rgba.length; i += 4)
   {
      let r = image.rgba[i];
      let g = image.rgba[i + 1];
      let b = image.rgba[i + 2];
      if (r < 16 && g < 16 && b < 16)
      {
         dark += 1;
      }
      if (Math.max(r, g, b) - Math.min(r, g, b) > 72)
      {
         colorful += 1;
      }
      if (r > 210 && g > 210 && b > 210)
      {
         bright += 1;
      }
   }
   if (dark < 100000 || colorful < 10000 || bright < 1000)
   {
      throw new Error(`capture does not look like the local-layer scene: dark=${dark} colorful=${colorful} bright=${bright}`);
   }
}

function assertPreparedRendered(image)
{
   let dark = 0;
   let colorful = 0;
   let bright = 0;
   for (let i = 0; i < image.rgba.length; i += 4) {
      let r = image.rgba[i];
      let g = image.rgba[i + 1];
      let b = image.rgba[i + 2];
      if (r < 16 && g < 16 && b < 16) {
         dark += 1;
      }
      if (Math.max(r, g, b) - Math.min(r, g, b) > 72) {
         colorful += 1;
      }
      if (r > 210 && g > 210 && b > 210) {
         bright += 1;
      }
   }
   if (dark < 500000 || colorful < 10000 || bright < 10000) {
      throw new Error(`capture does not look like the prepared-chunk scene: dark=${dark} colorful=${colorful} bright=${bright}`);
   }
}

function assertGlyphRendered(image)
{
   let dark = 0;
   let bright = 0;
   let cyan = 0;
   for (let i = 0; i < image.rgba.length; i += 4) {
      let r = image.rgba[i];
      let g = image.rgba[i + 1];
      let b = image.rgba[i + 2];
      if (r < 24 && g < 28 && b < 36) {
         dark += 1;
      }
      if (r > 180 && g > 180 && b > 180) {
         bright += 1;
      }
      if (b > 180 && g > 150 && r < 180) {
         cyan += 1;
      }
   }
   if (dark < 100000 || bright < 5000 || cyan < 1000) {
      throw new Error(`capture does not look like the A8/SDF glyph scene: dark=${dark} bright=${bright} cyan=${cyan}`);
   }
}

function assertAppRendered(image)
{
   let blue = 0;
   let dark = 0;
   let background = 0;
   for (let i = 0; i < image.rgba.length; i += 4) {
      let r = image.rgba[i];
      let g = image.rgba[i + 1];
      let b = image.rgba[i + 2];
      if (b > 180 && r < 120 && g > 80) {
         blue += 1;
      }
      if (r < 16 && g < 16 && b < 16) {
         dark += 1;
      }
      if (r > 235 && g > 235 && b > 235) {
         background += 1;
      }
   }
   if (blue < 4000 || dark < 3000 || background < 20000) {
      throw new Error(`capture does not look like the Oxide WebGPU scene: blue=${blue} dark=${dark} background=${background}`);
   }
}

function assertIdMaskRendered(image)
{
   let colorful = 0;
   let green = 0;
   let blue = 0;
   let bright = 0;
   let dark = 0;
   for (let i = 0; i < image.rgba.length; i += 4) {
      let r = image.rgba[i];
      let g = image.rgba[i + 1];
      let b = image.rgba[i + 2];
      let hi = Math.max(r, g, b);
      let lo = Math.min(r, g, b);
      if (hi - lo > 48) {
         colorful += 1;
      }
      if (g > r + 16 && g > b + 16) {
         green += 1;
      }
      if (b > r + 20 && b > g + 20) {
         blue += 1;
      }
      if (r > 180 || g > 180 || b > 180) {
         bright += 1;
      }
      if (r < 24 && g < 24 && b < 24) {
         dark += 1;
      }
   }
   if (colorful < 100000 || green < 25000 || blue < 50000 || bright < 5000 || bright > 80000 || dark > 80000) {
      throw new Error(`capture does not look like the WebGPU ID-mask compositor: colorful=${colorful} green=${green} blue=${blue} bright=${bright} dark=${dark}`);
   }
}

function assertScene3dRendered(image)
{
   let colorful = 0;
   let blue = 0;
   let orange = 0;
   let dark = 0;
   for (let i = 0; i < image.rgba.length; i += 4) {
      let r = image.rgba[i];
      let g = image.rgba[i + 1];
      let b = image.rgba[i + 2];
      let hi = Math.max(r, g, b);
      let lo = Math.min(r, g, b);
      if (hi > 48 && hi - lo > 36) {
         colorful += 1;
      }
      if (b > r + 36 && b > g + 16) {
         blue += 1;
      }
      if (r > b + 36 && g > b + 8) {
         orange += 1;
      }
      if (r < 24 && g < 24 && b < 32) {
         dark += 1;
      }
   }
   if (colorful < 25000 || blue < 9000 || orange < 5000 || dark < 100000) {
      throw new Error(`capture does not look like the WebGPU Scene3D frame: colorful=${colorful} blue=${blue} orange=${orange} dark=${dark}`);
   }
}

function parseMetricString(text)
{
   let metrics = {};
   for (let part of String(text || "").split(";")) {
      if (!part) {
         continue;
      }
      let split = part.indexOf("=");
      if (split <= 0) {
         continue;
      }
      metrics[part.slice(0, split)] = part.slice(split + 1);
   }
   return metrics;
}

function numberMetric(metrics, key)
{
   let value = Number(metrics[key]);
   if (!Number.isFinite(value)) {
      throw new Error(`missing numeric browser metric ${key}`);
   }
   return value;
}

function stringMetric(metrics, key)
{
   let value = metrics[key];
   if (typeof value !== "string" || value.length === 0) {
      throw new Error(`missing string browser metric ${key}`);
   }
   return value;
}

function webPackageFile(kind, rel)
{
   let path = join(webRoot, rel);
   let bytes = statSync(path).size;
   if (!Number.isFinite(bytes) || bytes <= 0) {
      throw new Error(`missing browser package artifact ${rel}`);
   }
   return {
      kind,
      path: `host/web-app/www/${rel}`,
      bytes,
   };
}

function webPackageStats()
{
   let files = [
      webPackageFile("wasm", "pkg/oxide_host_web_bg.wasm"),
      webPackageFile("js", "pkg/oxide_host_web.js"),
      webPackageFile("typescript", "pkg/oxide_host_web.d.ts"),
      webPackageFile("wasm_typescript", "pkg/oxide_host_web_bg.wasm.d.ts"),
   ];
   let packageBytes = files.reduce((sum, file) => sum + file.bytes, 0);
   let byKind = new Map(files.map(file => [file.kind, file.bytes]));
   return {
      package_root: "host/web-app/www/pkg",
      package_file_count: files.length,
      package_bytes: packageBytes,
      wasm_bytes: byKind.get("wasm"),
      js_bytes: byKind.get("js"),
      typescript_bytes: byKind.get("typescript"),
      wasm_typescript_bytes: byKind.get("wasm_typescript"),
      files,
   };
}

function browserStartupNumber(startup, key)
{
   let value = Number(startup[key]);
   if (!Number.isFinite(value) || value < 0.0) {
      throw new Error(`missing browser startup metric ${key}`);
   }
   return value;
}

function browserStartupSummary(pageReport)
{
   let startup = pageReport.browser_startup;
   if (!startup || typeof startup !== "object") {
      throw new Error("web report missing browser startup metrics");
   }
   if (startup.id !== "web.wasm.webgpu.browser_startup") {
      throw new Error("web report has unexpected browser startup id");
   }
   let packageStats = webPackageStats();
   return {
      id: startup.id,
      source: "performance.now+node.fs.stat",
      page_start_ms: browserStartupNumber(startup, "page_start_ms"),
      wasm_init_start_ms: browserStartupNumber(startup, "wasm_init_start_ms"),
      wasm_init_ms: browserStartupNumber(startup, "wasm_init_ms"),
      app_init_start_ms: browserStartupNumber(startup, "app_init_start_ms"),
      app_init_ms: browserStartupNumber(startup, "app_init_ms"),
      first_frame_start_ms: browserStartupNumber(startup, "first_frame_start_ms"),
      first_frame_ms: browserStartupNumber(startup, "first_frame_ms"),
      report_ready_ms: browserStartupNumber(startup, "report_ready_ms"),
      wasm_memory_bytes: browserStartupNumber(startup, "wasm_memory_bytes"),
      ...packageStats,
   };
}

function percentile(sorted, rank)
{
   if (sorted.length === 0) {
      return 0.0;
   }
   let index = Math.min(sorted.length - 1, Math.max(0, Math.ceil(sorted.length * rank) - 1));
   return sorted[index];
}

function distribution(values)
{
   let sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
   if (sorted.length === 0) {
      throw new Error("startup distribution has no finite samples");
   }
   return {
      samples: sorted.length,
      min: sorted[0],
      p50: percentile(sorted, 0.50),
      p95: percentile(sorted, 0.95),
      p99: percentile(sorted, 0.99),
      max: sorted[sorted.length - 1],
      avg: sorted.reduce((sum, value) => sum + value, 0.0) / sorted.length,
   };
}

function startupSample(pageReport, index)
{
   let startup = browserStartupSummary(pageReport);
   if (pageReport.backend !== "webgpu") {
      throw new Error(`startup sample ${index} did not initialize WebGPU backend`);
   }
   let renderMetrics = parseMetricString(pageReport.render);
   if (numberMetric(renderMetrics, "draws") <= 0) {
      throw new Error(`startup sample ${index} did not render a visible WebGPU frame`);
   }
   return {
      index,
      backend: pageReport.backend,
      render: pageReport.render,
      ...startup,
   };
}

function startupRepeatReport(args, samples)
{
   let keys = [
      "wasm_init_ms",
      "app_init_ms",
      "first_frame_ms",
      "report_ready_ms",
      "wasm_memory_bytes",
   ];
   let summaries = {};
   for (let key of keys) {
      summaries[key] = distribution(samples.map(sample => sample[key]));
   }
   return {
      version: 1,
      id: "web.wasm.webgpu.browser_startup_repeats",
      source: "performance.now+node.fs.stat",
      repeats: args.startupRepeats,
      package: webPackageStats(),
      summaries,
      samples,
   };
}

async function writeStartupReport(args, baseUrl, nextReportPromise)
{
   let samples = [];
   let startupUrl = browserUrl(args, baseUrl, true, true);
   for (let index = 0; index < args.startupRepeats; index += 1) {
      let pageReport = await runChromeForReport(args, startupUrl, nextReportPromise());
      samples.push(startupSample(pageReport, index));
   }
   let report = startupRepeatReport(args, samples);
   mkdirSync(dirname(args.startupReport), { recursive: true });
   writeFileSync(args.startupReport, `${JSON.stringify(report, null, 2)}\n`);
   console.log(`wrote ${args.startupReport}`);
}

function canvasBrowserStartupSummary(pageReport)
{
   let startup = pageReport.browser_startup;
   if (!startup || typeof startup !== "object") {
      throw new Error("canvas report missing browser startup metrics");
   }
   if (startup.id !== "web.wasm.canvas.browser_startup") {
      throw new Error("canvas report has unexpected browser startup id");
   }
   return {
      id: startup.id,
      source: "performance.now+node.fs.stat",
      page_start_ms: browserStartupNumber(startup, "page_start_ms"),
      wasm_init_start_ms: browserStartupNumber(startup, "wasm_init_start_ms"),
      wasm_init_ms: browserStartupNumber(startup, "wasm_init_ms"),
      app_init_start_ms: browserStartupNumber(startup, "app_init_start_ms"),
      app_init_ms: browserStartupNumber(startup, "app_init_ms"),
      first_frame_start_ms: browserStartupNumber(startup, "first_frame_start_ms"),
      first_frame_ms: browserStartupNumber(startup, "first_frame_ms"),
      report_ready_ms: browserStartupNumber(startup, "report_ready_ms"),
      wasm_memory_bytes: browserStartupNumber(startup, "wasm_memory_bytes"),
      ...webPackageStats(),
   };
}

function canvasIndexedQuadCase(metrics)
{
   let samples = numberMetric(metrics, "samples");
   let framesPerSample = numberMetric(metrics, "frames_per_sample");
   let quads = numberMetric(metrics, "quads");
   let row = {
      id: "web.wasm.canvas.indexed_quads",
      layer: "engine",
      scenario: "browser-submit-throughput",
      variant: "canvas2d-indexed-image-mesh",
      cache_state: "warm",
      refresh_mode: "unpaced-tight-loop",
      samples,
      frames_per_sample: framesPerSample,
      frames: numberMetric(metrics, "frames"),
      p50_ms: numberMetric(metrics, "p50_ms"),
      p95_ms: numberMetric(metrics, "p95_ms"),
      p99_ms: numberMetric(metrics, "p99_ms"),
      peak_ms: numberMetric(metrics, "peak_ms"),
      avg_ms: numberMetric(metrics, "avg_ms"),
      ...allocationMetricFields(metrics, ""),
      draws: numberMetric(metrics, "draws"),
      draw_items: numberMetric(metrics, "draw_items"),
      draw_items_coalesced: numberMetric(metrics, "draw_items_coalesced"),
      draw_pipeline_binds: numberMetric(metrics, "draw_pipeline_binds"),
      draw_bind_group_binds: numberMetric(metrics, "draw_bind_group_binds"),
      draw_scissor_sets: numberMetric(metrics, "draw_scissor_sets"),
      solid_tris: numberMetric(metrics, "solid_tris"),
      rrect_instances: numberMetric(metrics, "rrect_instances"),
      rrect_triangles: numberMetric(metrics, "rrect_triangles"),
      rrect_instance_bytes: numberMetric(metrics, "rrect_instance_bytes"),
      image_draws: numberMetric(metrics, "image_draws"),
      image_mesh_draws: numberMetric(metrics, "image_mesh_draws"),
      nine_slice_draws: numberMetric(metrics, "nine_slice_draws"),
      glyph_quads: numberMetric(metrics, "glyph_quads"),
      sdf_glyph_quads: numberMetric(metrics, "sdf_glyph_quads"),
      clip_depth_peak: numberMetric(metrics, "clip_depth_peak"),
      damage_rects: numberMetric(metrics, "damage_rects"),
      render_passes: numberMetric(metrics, "render_passes"),
      clear_passes: numberMetric(metrics, "clear_passes"),
      draw_passes: numberMetric(metrics, "draw_passes"),
      present_passes: numberMetric(metrics, "present_passes"),
      texture_copies: numberMetric(metrics, "texture_copies"),
      command_buffers: numberMetric(metrics, "command_buffers"),
      buffer_upload_bytes: numberMetric(metrics, "buffer_upload_bytes"),
      texture_upload_bytes: numberMetric(metrics, "texture_upload_bytes"),
      buffer_grows: numberMetric(metrics, "buffer_grows"),
      texture_creates: numberMetric(metrics, "texture_creates"),
      bind_group_creates: numberMetric(metrics, "bind_group_creates"),
      pipeline_creates: numberMetric(metrics, "pipeline_creates"),
      sampler_creates: numberMetric(metrics, "sampler_creates"),
      image_texture_creates: numberMetric(metrics, "image_texture_creates"),
      image_bind_group_creates: numberMetric(metrics, "image_bind_group_creates"),
      cpu_scratch_bytes: numberMetric(metrics, "cpu_scratch_bytes"),
      cpu_scratch_grows: numberMetric(metrics, "cpu_scratch_grows"),
      cpu_scratch_growth_bytes: numberMetric(metrics, "cpu_scratch_growth_bytes"),
      expected_image_meshes: numberMetric(metrics, "expected_image_meshes"),
      expected_image_draws: numberMetric(metrics, "expected_image_draws"),
      quads,
      unit: "ms/cpu-submit",
   };
   if (row.frames !== samples * framesPerSample) {
      throw new Error("canvas diagnostic frame count does not match samples * frames_per_sample");
   }
   if (row.expected_image_meshes !== 1 || row.image_mesh_draws !== 1) {
      throw new Error("canvas diagnostic did not exercise one indexed image mesh");
   }
   if (row.expected_image_draws !== quads || row.image_draws !== quads) {
      throw new Error("canvas diagnostic did not draw the requested indexed quads");
   }
   if (row.wasm_alloc_count < 0 || row.wasm_alloc_bytes < 0 || row.wasm_realloc_count < 0) {
      throw new Error("canvas diagnostic has invalid Rust/WASM allocation counters");
   }
   return row;
}

function canvasBenchmarkMarkSummary(pageReport)
{
   let marks = Array.isArray(pageReport.benchmark_marks) ? pageReport.benchmark_marks : [];
   let mark = marks.find(entry => entry && entry.id === "canvas_indexed_quads");
   if (!mark) {
      throw new Error("canvas report missing canvas_indexed_quads benchmark mark");
   }
   let durationMs = Number(mark.duration_ms);
   let startMs = Number(mark.start_ms);
   let wasmBeforeBytes = Number(mark.wasm_memory_before_bytes);
   let wasmAfterBytes = Number(mark.wasm_memory_after_bytes);
   let wasmGrowthBytes = Number(mark.wasm_memory_growth_bytes);
   let jsHeapBeforeBytes = Number(mark.js_heap_before_bytes);
   let jsHeapAfterBytes = Number(mark.js_heap_after_bytes);
   let jsHeapGrowthBytes = Number(mark.js_heap_growth_bytes);
   for (let [label, value] of [
      ["duration_ms", durationMs],
      ["start_ms", startMs],
      ["wasm_memory_before_bytes", wasmBeforeBytes],
      ["wasm_memory_after_bytes", wasmAfterBytes],
      ["wasm_memory_growth_bytes", wasmGrowthBytes],
      ["js_heap_before_bytes", jsHeapBeforeBytes],
      ["js_heap_after_bytes", jsHeapAfterBytes],
      ["js_heap_growth_bytes", jsHeapGrowthBytes],
   ]) {
      if (!Number.isFinite(value) || value < 0.0) {
         throw new Error(`canvas report benchmark mark missing finite ${label}`);
      }
   }
   if (durationMs <= 0.0 || wasmBeforeBytes <= 0.0 || wasmAfterBytes < wasmBeforeBytes) {
      throw new Error("canvas report benchmark mark has invalid timing or memory ordering");
   }
   return {
      id: "web.wasm.canvas.benchmark_mark_coverage",
      expected_count: 1,
      page_mark_count: marks.length,
      expected: ["canvas_indexed_quads"],
      page_labels: marks.map(entry => entry.id),
      marks: [
         {
            id: mark.id,
            name: typeof mark.name === "string" ? mark.name : "",
            start_ms: startMs,
            duration_ms: durationMs,
            wasm_memory_before_bytes: wasmBeforeBytes,
            wasm_memory_after_bytes: wasmAfterBytes,
            wasm_memory_growth_bytes: wasmGrowthBytes,
            js_heap_sample_supported: Number(mark.js_heap_sample_supported),
            js_heap_gc_available: Number(mark.js_heap_gc_available),
            js_heap_before_bytes: jsHeapBeforeBytes,
            js_heap_after_bytes: jsHeapAfterBytes,
            js_heap_growth_bytes: jsHeapGrowthBytes,
         },
      ],
   };
}

function canvasDiagnosticSample(pageReport, index)
{
   if (pageReport?.benchmark_error) {
      let error = pageReport.benchmark_error;
      throw new Error(`canvas diagnostic failed during ${error.id}: ${error.detail}`);
   }
   if (pageReport.backend !== "canvas2d") {
      throw new Error(`canvas diagnostic sample ${index} did not use Canvas2D backend`);
   }
   let row = canvasIndexedQuadCase(parseMetricString(pageReport.canvas_diag));
   return {
      index,
      browser_startup: canvasBrowserStartupSummary(pageReport),
      benchmark_marks: canvasBenchmarkMarkSummary(pageReport),
      case: row,
   };
}

function canvasDiagnosticReport(args, url, samples)
{
   let cases = samples.map(sample => sample.case);
   let summarize = key => distribution(cases.map(row => row[key]));
   return {
      version: 1,
      suite: "web-wasm-canvas-diagnostic",
      generated_date: args.reportDate,
      browser_target: args.chromeArch
         ? `Chrome ${args.chromeArch} via headless CLI`
         : "Chrome via headless CLI",
      url,
      status: "diagnostic-baseline",
      notes: [
         "This non-default Canvas2D diagnostic exercises the indexed ImageMesh quad walker for same-workload A/B proof before renderer changes.",
         "It is not part of the committed default WebGPU browser baseline and does not change production WebGPU startup behavior.",
      ],
      workload: {
         samples: args.canvasSamples,
         frames_per_sample: args.canvasFrames,
         quads: args.canvasQuads,
      },
      package: webPackageStats(),
      summaries: {
         p50_ms: summarize("p50_ms"),
         p95_ms: summarize("p95_ms"),
         p99_ms: summarize("p99_ms"),
         peak_ms: summarize("peak_ms"),
         avg_ms: summarize("avg_ms"),
         wasm_alloc_count: summarize("wasm_alloc_count"),
         wasm_alloc_bytes: summarize("wasm_alloc_bytes"),
         wasm_realloc_count: summarize("wasm_realloc_count"),
         wasm_peak_frame_alloc_bytes: summarize("wasm_peak_frame_alloc_bytes"),
      },
      repeats: args.canvasRepeats,
      samples,
   };
}

async function writeCanvasDiagnosticReport(args, baseUrl, nextReportPromise)
{
   let samples = [];
   let canvasUrl = browserUrl(args, baseUrl, true, false, true);
   for (let index = 0; index < args.canvasRepeats; index += 1) {
      let pageReport = await runChromeForReport(args, canvasUrl, nextReportPromise());
      samples.push(canvasDiagnosticSample(pageReport, index));
   }
   let report = canvasDiagnosticReport(args, persistedCanvasBrowserUrl(args), samples);
   mkdirSync(dirname(args.canvasReport), { recursive: true });
   writeFileSync(args.canvasReport, `${JSON.stringify(report, null, 2)}\n`);
   console.log(`wrote ${args.canvasReport}`);
}

function cpuSubmitCase(metrics)
{
   return {
      id: "web.wasm.webgpu.cpu_submit_throughput",
      layer: "engine",
      scenario: "browser-submit-throughput",
      variant: "webgpu-synchronous",
      cache_state: "warm",
      refresh_mode: "unpaced-tight-loop",
      samples: numberMetric(metrics, "samples"),
      frames_per_sample: numberMetric(metrics, "frames_per_sample"),
      frames: numberMetric(metrics, "frames"),
      cpu_submit_p50_ms: numberMetric(metrics, "cpu_submit_p50_ms"),
      cpu_submit_p95_ms: numberMetric(metrics, "cpu_submit_p95_ms"),
      cpu_submit_p99_ms: numberMetric(metrics, "cpu_submit_p99_ms"),
      cpu_submit_peak_ms: numberMetric(metrics, "cpu_submit_peak_ms"),
      cpu_submit_avg_ms: numberMetric(metrics, "cpu_submit_avg_ms"),
      ...allocationMetricFields(metrics, ""),
      ...submitAllocationMetricFields(metrics, ""),
      ...frameStageAllocationMetricFields(metrics),
      draws: numberMetric(metrics, "draws"),
      draw_items: numberMetric(metrics, "draw_items"),
      draw_items_coalesced: numberMetric(metrics, "draw_items_coalesced"),
      draw_pipeline_binds: numberMetric(metrics, "draw_pipeline_binds"),
      draw_bind_group_binds: numberMetric(metrics, "draw_bind_group_binds"),
      draw_scissor_sets: numberMetric(metrics, "draw_scissor_sets"),
      solid_tris: numberMetric(metrics, "solid_tris"),
      rrect_instances: numberMetric(metrics, "rrect_instances"),
      rrect_triangles: numberMetric(metrics, "rrect_triangles"),
      rrect_instance_bytes: numberMetric(metrics, "rrect_instance_bytes"),
      image_draws: numberMetric(metrics, "image_draws"),
      image_mesh_draws: numberMetric(metrics, "image_mesh_draws"),
      nine_slice_draws: numberMetric(metrics, "nine_slice_draws"),
      glyph_quads: numberMetric(metrics, "glyph_quads"),
      sdf_glyph_quads: numberMetric(metrics, "sdf_glyph_quads"),
      clip_depth_peak: numberMetric(metrics, "clip_depth_peak"),
      damage_rects: numberMetric(metrics, "damage_rects"),
      layer_draws: numberMetric(metrics, "layer_draws"),
      layer_cache_hits: numberMetric(metrics, "layer_cache_hits"),
      layer_cache_misses: numberMetric(metrics, "layer_cache_misses"),
      layer_cache_skipped_draws: numberMetric(metrics, "layer_cache_skipped_draws"),
      layer_passes: numberMetric(metrics, "layer_passes"),
      scene3d_draws: numberMetric(metrics, "scene3d_draws"),
      id_mask_draws: numberMetric(metrics, "id_mask_draws"),
      backdrop_draws: numberMetric(metrics, "backdrop_draws"),
      visual_effect_draws: numberMetric(metrics, "visual_effect_draws"),
      effect_uniform_writes: numberMetric(metrics, "effect_uniform_writes"),
      effect_uniform_bytes: numberMetric(metrics, "effect_uniform_bytes"),
      effect_uniform_slots: numberMetric(metrics, "effect_uniform_slots"),
      id_mask_uniform_writes: numberMetric(metrics, "id_mask_uniform_writes"),
      id_mask_uniform_bytes: numberMetric(metrics, "id_mask_uniform_bytes"),
      id_mask_uniform_slots: numberMetric(metrics, "id_mask_uniform_slots"),
      spinner_draws: numberMetric(metrics, "spinner_draws"),
      camera_bg_draws: numberMetric(metrics, "camera_bg_draws"),
      render_passes: numberMetric(metrics, "render_passes"),
      clear_passes: numberMetric(metrics, "clear_passes"),
      draw_passes: numberMetric(metrics, "draw_passes"),
      scene3d_passes: numberMetric(metrics, "scene3d_passes"),
      scene3d_overlay_passes: numberMetric(metrics, "scene3d_overlay_passes"),
      id_mask_raster_passes: numberMetric(metrics, "id_mask_raster_passes"),
      id_mask_field_seed_passes: numberMetric(metrics, "id_mask_field_seed_passes"),
      id_mask_field_jump_passes: numberMetric(metrics, "id_mask_field_jump_passes"),
      id_mask_compositor_passes: numberMetric(metrics, "id_mask_compositor_passes"),
      present_passes: numberMetric(metrics, "present_passes"),
      texture_copies: numberMetric(metrics, "texture_copies"),
      command_buffers: numberMetric(metrics, "command_buffers"),
      ...timestampMetricFields(metrics, ""),
      buffer_upload_bytes: numberMetric(metrics, "buffer_upload_bytes"),
      texture_upload_bytes: numberMetric(metrics, "texture_upload_bytes"),
      ...resourceMetricFields(metrics, ""),
      image_upload_temp_allocs: numberMetric(metrics, "image_upload_temp_allocs"),
      image_upload_temp_bytes: numberMetric(metrics, "image_upload_temp_bytes"),
      image_upload_scratch_bytes: numberMetric(metrics, "image_upload_scratch_bytes"),
      image_upload_scratch_grows: numberMetric(metrics, "image_upload_scratch_grows"),
      ...scratchMetricFields(metrics, ""),
      unit: "ms/cpu-submit",
   };
}

function rawPacingFields(values, refreshHz)
{
   let budgetMs = 1000.0 / refreshHz;
   let missedFrames = values.filter(value => value > budgetMs).length;
   let hitchFrames = values.filter(value => value > budgetMs * 2.0).length;
   return {
      [`frame_budget_${refreshHz}hz_ms`]: budgetMs,
      [`missed_frames_${refreshHz}hz`]: missedFrames,
      [`missed_frame_ratio_${refreshHz}hz`]: missedFrames / values.length,
      [`hitch_frames_${refreshHz}hz`]: hitchFrames,
      [`hitch_ratio_${refreshHz}hz`]: hitchFrames / values.length,
   };
}

function rafFrameCase(raw)
{
   if (!raw || typeof raw !== "object") {
      throw new Error("web report missing RAF frame evidence");
   }
   let frames = Number(raw.frames);
   let submissions = Number(raw.submissions);
   let rafDeltas = Array.isArray(raw.raf_deltas_ms) ? raw.raf_deltas_ms : [];
   let rafTimestamps = Array.isArray(raw.raf_timestamps_ms) ? raw.raf_timestamps_ms : [];
   let cpuSubmit = Array.isArray(raw.cpu_submit_ms) ? raw.cpu_submit_ms : [];
   let warmupCpuSubmit = Array.isArray(raw.warmup_cpu_submit_ms) ? raw.warmup_cpu_submit_ms : [];
   let instrumentationEnabled = raw.instrumentation_overhead?.enabled_ms;
   let instrumentationDisabled = raw.instrumentation_overhead?.disabled_ms;
   if (!Number.isInteger(frames) || frames < 2000
      || submissions !== frames
      || rafDeltas.length !== frames
      || rafTimestamps.length !== frames
      || cpuSubmit.length !== frames) {
      throw new Error("RAF evidence must contain one timestamp, delta, CPU sample, and submission per frame");
   }
   if (warmupCpuSubmit.length !== Number(raw.warmup_frames)
      || !Array.isArray(instrumentationEnabled)
      || !Array.isArray(instrumentationDisabled)
      || instrumentationEnabled.length !== 200
      || instrumentationDisabled.length !== 200) {
      throw new Error("RAF evidence is missing warmup or balanced instrumentation-overhead samples");
   }
   let stageDistributions = {};
   let stageSamples = raw.cpu_stages_ms;
   if (!stageSamples || typeof stageSamples !== "object") {
      throw new Error("RAF evidence is missing CPU stage samples");
   }
   for (let name of RAF_CPU_STAGE_NAMES) {
      let values = stageSamples[name];
      if (!Array.isArray(values) || values.length !== frames) {
         throw new Error(`RAF CPU stage ${name} does not contain ${frames} samples`);
      }
      stageDistributions[name] = distribution(values);
   }
   let frameDistribution = distribution(rafDeltas);
   let cpuDistribution = distribution(cpuSubmit);
   let instrumentationEnabledDistribution = distribution(instrumentationEnabled);
   let instrumentationDisabledDistribution = distribution(instrumentationDisabled);
   let gpuSamples = Array.isArray(raw.gpu_timestamp_samples) ? raw.gpu_timestamp_samples : [];
   let gpuDurationMs = gpuSamples.map(sample => Number(sample.total_ns) / 1_000_000.0);
   let gpuDistribution;
   if (raw.gpu_timestamp_status === "collected") {
      if (gpuDurationMs.length < 2000 || gpuDurationMs.some(value => !Number.isFinite(value))) {
         throw new Error("RAF evidence has fewer than 2000 valid GPU timestamp samples");
      }
      gpuDistribution = distribution(gpuDurationMs);
   } else {
      gpuDistribution = null;
   }
   return {
      id: "web.wasm.webgpu.raf_frame_loop",
      layer: "flow",
      scenario: "browser-displayed-frame",
      variant: "webgpu-production-raf",
      cache_state: "warm",
      refresh_mode: "browser-raf-native",
      samples: frames,
      frames,
      submissions,
      warmup_frames: Number(raw.warmup_frames),
      warmup_cpu_submit_ms: warmupCpuSubmit,
      instrumentation_overhead_order: raw.instrumentation_overhead.order,
      instrumentation_enabled_ms: instrumentationEnabled,
      instrumentation_disabled_ms: instrumentationDisabled,
      instrumentation_enabled_distribution_ms: instrumentationEnabledDistribution,
      instrumentation_disabled_distribution_ms: instrumentationDisabledDistribution,
      instrumentation_overhead_p50_ms:
         instrumentationEnabledDistribution.p50 - instrumentationDisabledDistribution.p50,
      p50_ms: frameDistribution.p50,
      p95_ms: frameDistribution.p95,
      p99_ms: frameDistribution.p99,
      peak_ms: frameDistribution.max,
      avg_ms: frameDistribution.avg,
      cpu_submit_p50_ms: cpuDistribution.p50,
      cpu_submit_p95_ms: cpuDistribution.p95,
      cpu_submit_p99_ms: cpuDistribution.p99,
      cpu_submit_peak_ms: cpuDistribution.max,
      ...rawPacingFields(rafDeltas, 60),
      ...rawPacingFields(rafDeltas, 120),
      raf_timestamps_ms: rafTimestamps,
      raf_deltas_ms: rafDeltas,
      cpu_submit_ms: cpuSubmit,
      cpu_stage_samples_ms: stageSamples,
      cpu_stage_distributions_ms: stageDistributions,
      cpu_stage_attribution: raw.cpu_stage_attribution,
      long_task_supported: Number(raw.long_task_supported),
      long_tasks: Array.isArray(raw.long_tasks) ? raw.long_tasks : [],
      event_to_submit_status: String(raw.event_to_submit_status),
      event_to_visible_status: String(raw.event_to_visible_status),
      gpu_timestamp_status: String(raw.gpu_timestamp_status),
      gpu_timestamp_readback_skips: Number(raw.gpu_timestamp_readback_skips),
      queue_drain_ms: Number(raw.queue_drain_ms),
      queue_drain_raf_waits: Number(raw.queue_drain_raf_waits),
      queue_pending_initial: Number(raw.queue_pending_initial),
      queue_pending_final: Number(raw.queue_pending_final),
      gpu_timestamp_samples: gpuSamples,
      gpu_ms: gpuDurationMs,
      gpu_ms_p50: gpuDistribution?.p50 ?? null,
      gpu_ms_p95: gpuDistribution?.p95 ?? null,
      gpu_ms_p99: gpuDistribution?.p99 ?? null,
      gpu_ms_peak: gpuDistribution?.max ?? null,
      production_path: Number(raw.production_path),
      production_coalescing: String(raw.production_coalescing),
      production_damage_policy: String(raw.production_damage_policy),
      submissions_per_raf: Number(raw.submissions_per_raf),
      scene_index: Number(raw.scene_index),
      resize_every_frames: Number(raw.resize_every_frames),
      viewport_css: String(raw.viewport_css),
      device_pixel_ratio: Number(raw.device_pixel_ratio),
      cross_origin_isolated: Boolean(raw.cross_origin_isolated),
      unit: "ms/displayed-frame",
   };
}

function timestampMetricFields(metrics, prefix)
{
   let key = name => `${prefix}${name}`;
   return {
      gpu_timestamp_query_supported: numberMetric(metrics, key("gpu_timestamp_query_supported")),
      gpu_timestamp_frame_id: numberMetric(metrics, key("gpu_timestamp_frame_id")),
      gpu_timestamp_passes: numberMetric(metrics, key("gpu_timestamp_passes")),
      gpu_timestamp_total_ns: numberMetric(metrics, key("gpu_timestamp_total_ns")),
      gpu_timestamp_clear_ns: numberMetric(metrics, key("gpu_timestamp_clear_ns")),
      gpu_timestamp_draw_ns: numberMetric(metrics, key("gpu_timestamp_draw_ns")),
      gpu_timestamp_scene3d_ns: numberMetric(metrics, key("gpu_timestamp_scene3d_ns")),
      gpu_timestamp_scene3d_overlay_ns: numberMetric(metrics, key("gpu_timestamp_scene3d_overlay_ns")),
      gpu_timestamp_id_mask_raster_ns: numberMetric(metrics, key("gpu_timestamp_id_mask_raster_ns")),
      gpu_timestamp_id_mask_field_seed_ns: numberMetric(metrics, key("gpu_timestamp_id_mask_field_seed_ns")),
      gpu_timestamp_id_mask_field_jump_ns: numberMetric(metrics, key("gpu_timestamp_id_mask_field_jump_ns")),
      gpu_timestamp_id_mask_compositor_ns: numberMetric(metrics, key("gpu_timestamp_id_mask_compositor_ns")),
      gpu_timestamp_present_ns: numberMetric(metrics, key("gpu_timestamp_present_ns")),
      gpu_timestamp_max_pass_ns: numberMetric(metrics, key("gpu_timestamp_max_pass_ns")),
      gpu_timestamp_readback_skips: numberMetric(metrics, key("gpu_timestamp_readback_skips")),
      gpu_timestamp_readback_interval: numberMetric(metrics, key("gpu_timestamp_readback_interval")),
   };
}

function allocationMetricFields(metrics, prefix)
{
   let key = name => `${prefix}${name}`;
   return {
      wasm_alloc_count: numberMetric(metrics, key("wasm_alloc_count")),
      wasm_alloc_bytes: numberMetric(metrics, key("wasm_alloc_bytes")),
      wasm_dealloc_count: numberMetric(metrics, key("wasm_dealloc_count")),
      wasm_dealloc_bytes: numberMetric(metrics, key("wasm_dealloc_bytes")),
      wasm_realloc_count: numberMetric(metrics, key("wasm_realloc_count")),
      wasm_realloc_grow_bytes: numberMetric(metrics, key("wasm_realloc_grow_bytes")),
      wasm_realloc_shrink_bytes: numberMetric(metrics, key("wasm_realloc_shrink_bytes")),
      wasm_allocating_frames: numberMetric(metrics, key("wasm_allocating_frames")),
      wasm_peak_frame_alloc_bytes: numberMetric(metrics, key("wasm_peak_frame_alloc_bytes")),
   };
}

const WASM_SUBMIT_STAGE_NAMES = [
   "upload",
   "surface",
   "encoder",
   "render",
   "timestamp",
   "scratch_stats",
   "finish_queue",
   "present",
   "timestamp_map",
];

function submitAllocationMetricFields(metrics, prefix)
{
   let fields = {};
   for (let name of WASM_SUBMIT_STAGE_NAMES) {
      let key = `submit_${name}_`;
      fields[`${key}alloc_count`] = numberMetric(metrics, `${prefix}${key}alloc_count`);
      fields[`${key}alloc_bytes`] = numberMetric(metrics, `${prefix}${key}alloc_bytes`);
   }
   fields.submit_total_alloc_count = numberMetric(metrics, `${prefix}submit_total_alloc_count`);
   fields.submit_total_alloc_bytes = numberMetric(metrics, `${prefix}submit_total_alloc_bytes`);
   fields.submit_total_realloc_count = numberMetric(metrics, `${prefix}submit_total_realloc_count`);
   fields.submit_total_realloc_grow_bytes = numberMetric(
      metrics,
      `${prefix}submit_total_realloc_grow_bytes`,
   );
   return fields;
}

const WASM_FRAME_STAGE_NAMES = [
   "canvas_resize",
   "frame_timing",
   "builder_clear",
   "router_update",
   "router_draw",
   "damage_handoff",
   "draw_coalesce",
   "begin_frame",
   "encode_pass",
   "submit",
   "post_submit",
];

const RAF_CPU_STAGE_NAMES = [
   "event_update",
   "layout",
   "text_prepare",
   "draw_extraction",
   "coalescing",
   "backend_lowering",
   "upload",
   "command_encoding",
   "queue_submit",
   "post_submit_contract",
];

function frameStageAllocationMetricFields(metrics)
{
   let fields = {};
   for (let name of WASM_FRAME_STAGE_NAMES) {
      let prefix = `wasm_stage_${name}_`;
      fields[`${prefix}alloc_count`] = numberMetric(metrics, `${prefix}alloc_count`);
      fields[`${prefix}alloc_bytes`] = numberMetric(metrics, `${prefix}alloc_bytes`);
      fields[`${prefix}realloc_count`] = numberMetric(metrics, `${prefix}realloc_count`);
      fields[`${prefix}realloc_grow_bytes`] = numberMetric(metrics, `${prefix}realloc_grow_bytes`);
      fields[`${prefix}peak_frame_alloc_bytes`] = numberMetric(
         metrics,
         `${prefix}peak_frame_alloc_bytes`,
      );
   }
   return fields;
}

function resourceMetricFields(metrics, prefix)
{
   let key = name => `${prefix}${name}`;
   return {
      buffer_grows: numberMetric(metrics, key("buffer_grows")),
      texture_creates: numberMetric(metrics, key("texture_creates")),
      bind_group_creates: numberMetric(metrics, key("bind_group_creates")),
      pipeline_creates: numberMetric(metrics, key("pipeline_creates")),
      sampler_creates: numberMetric(metrics, key("sampler_creates")),
      mesh3d_creates: numberMetric(metrics, key("mesh3d_creates")),
      draw_buffer_grows: numberMetric(metrics, key("draw_buffer_grows")),
      image_texture_creates: numberMetric(metrics, key("image_texture_creates")),
      image_bind_group_creates: numberMetric(metrics, key("image_bind_group_creates")),
      target_texture_creates: numberMetric(metrics, key("target_texture_creates")),
      target_bind_group_creates: numberMetric(metrics, key("target_bind_group_creates")),
      layer_texture_creates: numberMetric(metrics, key("layer_texture_creates")),
      layer_bind_group_creates: numberMetric(metrics, key("layer_bind_group_creates")),
      scene3d_buffer_grows: numberMetric(metrics, key("scene3d_buffer_grows")),
      scene3d_bind_group_creates: numberMetric(metrics, key("scene3d_bind_group_creates")),
      effect_buffer_grows: numberMetric(metrics, key("effect_buffer_grows")),
      effect_bind_group_creates: numberMetric(metrics, key("effect_bind_group_creates")),
      id_mask_texture_creates: numberMetric(metrics, key("id_mask_texture_creates")),
      id_mask_buffer_grows: numberMetric(metrics, key("id_mask_buffer_grows")),
      id_mask_bind_group_creates: numberMetric(metrics, key("id_mask_bind_group_creates")),
   };
}

function scratchMetricFields(metrics, prefix)
{
   let key = name => `${prefix}${name}`;
   return {
      cpu_scratch_bytes: numberMetric(metrics, key("cpu_scratch_bytes")),
      cpu_scratch_grows: numberMetric(metrics, key("cpu_scratch_grows")),
      cpu_scratch_growth_bytes: numberMetric(metrics, key("cpu_scratch_growth_bytes")),
      cpu_draw_scratch_bytes: numberMetric(metrics, key("cpu_draw_scratch_bytes")),
      cpu_draw_scratch_grows: numberMetric(metrics, key("cpu_draw_scratch_grows")),
      cpu_draw_scratch_growth_bytes: numberMetric(metrics, key("cpu_draw_scratch_growth_bytes")),
      cpu_scene3d_scratch_bytes: numberMetric(metrics, key("cpu_scene3d_scratch_bytes")),
      cpu_scene3d_scratch_grows: numberMetric(metrics, key("cpu_scene3d_scratch_grows")),
      cpu_scene3d_scratch_growth_bytes: numberMetric(metrics, key("cpu_scene3d_scratch_growth_bytes")),
      cpu_effect_scratch_bytes: numberMetric(metrics, key("cpu_effect_scratch_bytes")),
      cpu_effect_scratch_grows: numberMetric(metrics, key("cpu_effect_scratch_grows")),
      cpu_effect_scratch_growth_bytes: numberMetric(metrics, key("cpu_effect_scratch_growth_bytes")),
      cpu_id_mask_scratch_bytes: numberMetric(metrics, key("cpu_id_mask_scratch_bytes")),
      cpu_id_mask_scratch_grows: numberMetric(metrics, key("cpu_id_mask_scratch_grows")),
      cpu_id_mask_scratch_growth_bytes: numberMetric(metrics, key("cpu_id_mask_scratch_growth_bytes")),
      cpu_image_upload_scratch_bytes: numberMetric(metrics, key("cpu_image_upload_scratch_bytes")),
      cpu_image_upload_scratch_grows: numberMetric(metrics, key("cpu_image_upload_scratch_grows")),
      cpu_image_upload_scratch_growth_bytes: numberMetric(metrics, key("cpu_image_upload_scratch_growth_bytes")),
      cpu_resource_table_scratch_bytes: numberMetric(metrics, key("cpu_resource_table_scratch_bytes")),
      cpu_resource_table_scratch_grows: numberMetric(metrics, key("cpu_resource_table_scratch_grows")),
      cpu_resource_table_scratch_growth_bytes: numberMetric(metrics, key("cpu_resource_table_scratch_growth_bytes")),
      commands_traversed: numberMetric(metrics, key("commands_traversed")),
      commands_copied: numberMetric(metrics, key("commands_copied")),
      geometry_bytes_copied: numberMetric(metrics, key("geometry_bytes_copied")),
      chunks_reused: numberMetric(metrics, key("chunks_reused")),
      chunks_rebuilt: numberMetric(metrics, key("chunks_rebuilt")),
      chunks_prepared: numberMetric(metrics, key("chunks_prepared")),
      backend_cache_hits: numberMetric(metrics, key("backend_cache_hits")),
      backend_cache_misses: numberMetric(metrics, key("backend_cache_misses")),
      render_encoders: numberMetric(metrics, key("render_encoders")),
      render_bundle_creates: numberMetric(metrics, key("render_bundle_creates")),
      render_bundle_replays: numberMetric(metrics, key("render_bundle_replays")),
      render_bundle_draws: numberMetric(metrics, key("render_bundle_draws")),
      prepared_direct_draws: numberMetric(metrics, key("prepared_direct_draws")),
      texture_copy_pixels: numberMetric(metrics, key("texture_copy_pixels")),
      texture_copy_bytes: numberMetric(metrics, key("texture_copy_bytes")),
      shaded_damage_pixels: numberMetric(metrics, key("shaded_damage_pixels")),
      cache_evictions: numberMetric(metrics, key("cache_evictions")),
      wakeups: numberMetric(metrics, key("wakeups")),
      skipped_submissions: numberMetric(metrics, key("skipped_submissions")),
      actual_submissions: numberMetric(metrics, key("actual_submissions")),
      gpu_allocated_bytes_available: numberMetric(metrics, key("gpu_allocated_bytes_available")),
      gpu_logical_total_bytes: numberMetric(metrics, key("gpu_logical_total_bytes")),
      gpu_allocated_total_bytes: numberMetric(metrics, key("gpu_allocated_total_bytes")),
      gpu_vertex_buffer_bytes: numberMetric(metrics, key("gpu_vertex_buffer_bytes")),
      gpu_index_buffer_bytes: numberMetric(metrics, key("gpu_index_buffer_bytes")),
      gpu_uniform_buffer_bytes: numberMetric(metrics, key("gpu_uniform_buffer_bytes")),
      gpu_persistent_asset_bytes: numberMetric(metrics, key("gpu_persistent_asset_bytes")),
      gpu_transient_target_bytes: numberMetric(metrics, key("gpu_transient_target_bytes")),
      gpu_depth_target_bytes: numberMetric(metrics, key("gpu_depth_target_bytes")),
      gpu_bloom_target_bytes: numberMetric(metrics, key("gpu_bloom_target_bytes")),
      gpu_layer_texture_bytes: numberMetric(metrics, key("gpu_layer_texture_bytes")),
      gpu_id_mask_texture_bytes: numberMetric(metrics, key("gpu_id_mask_texture_bytes")),
      gpu_atlas_texture_bytes: numberMetric(metrics, key("gpu_atlas_texture_bytes")),
      gpu_image_texture_bytes: numberMetric(metrics, key("gpu_image_texture_bytes")),
      gpu_scene3d_mesh_bytes: numberMetric(metrics, key("gpu_scene3d_mesh_bytes")),
      gpu_staging_buffer_bytes: numberMetric(metrics, key("gpu_staging_buffer_bytes")),
      gpu_bind_buffer_bytes: numberMetric(metrics, key("gpu_bind_buffer_bytes")),
      gpu_frame_ring_bytes: numberMetric(metrics, key("gpu_frame_ring_bytes")),
      gpu_cache_bytes: numberMetric(metrics, key("gpu_cache_bytes")),
   };
}

function idMaskCase(metrics, id, variant, prefix)
{
   let samples = numberMetric(metrics, "samples");
   let framesPerSample = numberMetric(metrics, "frames_per_sample");
   return {
      id,
      layer: "engine",
      scenario: "browser-submit-throughput",
      variant,
      cache_state: "warm",
      refresh_mode: "unpaced-tight-loop",
      samples,
      frames_per_sample: framesPerSample,
      frames: samples * framesPerSample,
      warmup_ms: numberMetric(metrics, `${prefix}_warmup_ms`),
      p50_ms: numberMetric(metrics, `${prefix}_p50_ms`),
      p95_ms: numberMetric(metrics, `${prefix}_p95_ms`),
      p99_ms: numberMetric(metrics, `${prefix}_p99_ms`),
      peak_ms: numberMetric(metrics, `${prefix}_peak_ms`),
      avg_ms: numberMetric(metrics, `${prefix}_avg_ms`),
      ...allocationMetricFields(metrics, `${prefix}_`),
      ...submitAllocationMetricFields(metrics, `${prefix}_`),
      draws: numberMetric(metrics, `${prefix}_draws`),
      draw_items: numberMetric(metrics, `${prefix}_draw_items`),
      draw_items_coalesced: numberMetric(metrics, `${prefix}_draw_items_coalesced`),
      draw_pipeline_binds: numberMetric(metrics, `${prefix}_draw_pipeline_binds`),
      draw_bind_group_binds: numberMetric(metrics, `${prefix}_draw_bind_group_binds`),
      draw_scissor_sets: numberMetric(metrics, `${prefix}_draw_scissor_sets`),
      solid_tris: numberMetric(metrics, `${prefix}_solid_tris`),
      rrect_instances: numberMetric(metrics, `${prefix}_rrect_instances`),
      rrect_triangles: numberMetric(metrics, `${prefix}_rrect_triangles`),
      rrect_instance_bytes: numberMetric(metrics, `${prefix}_rrect_instance_bytes`),
      image_draws: numberMetric(metrics, `${prefix}_image_draws`),
      image_mesh_draws: numberMetric(metrics, `${prefix}_image_mesh_draws`),
      nine_slice_draws: numberMetric(metrics, `${prefix}_nine_slice_draws`),
      glyph_quads: numberMetric(metrics, `${prefix}_glyph_quads`),
      sdf_glyph_quads: numberMetric(metrics, `${prefix}_sdf_glyph_quads`),
      clip_depth_peak: numberMetric(metrics, `${prefix}_clip_depth_peak`),
      damage_rects: numberMetric(metrics, `${prefix}_damage_rects`),
      layer_draws: numberMetric(metrics, `${prefix}_layer_draws`),
      layer_cache_hits: numberMetric(metrics, `${prefix}_layer_cache_hits`),
      layer_cache_misses: numberMetric(metrics, `${prefix}_layer_cache_misses`),
      layer_cache_skipped_draws: numberMetric(metrics, `${prefix}_layer_cache_skipped_draws`),
      layer_passes: numberMetric(metrics, `${prefix}_layer_passes`),
      scene3d_draws: numberMetric(metrics, `${prefix}_scene3d_draws`),
      id_mask_draws: numberMetric(metrics, `${prefix}_id_mask_draws`),
      backdrop_draws: numberMetric(metrics, `${prefix}_backdrop_draws`),
      visual_effect_draws: numberMetric(metrics, `${prefix}_visual_effect_draws`),
      effect_uniform_writes: numberMetric(metrics, `${prefix}_effect_uniform_writes`),
      effect_uniform_bytes: numberMetric(metrics, `${prefix}_effect_uniform_bytes`),
      effect_uniform_slots: numberMetric(metrics, `${prefix}_effect_uniform_slots`),
      id_mask_uniform_writes: numberMetric(metrics, `${prefix}_id_mask_uniform_writes`),
      id_mask_uniform_bytes: numberMetric(metrics, `${prefix}_id_mask_uniform_bytes`),
      id_mask_uniform_slots: numberMetric(metrics, `${prefix}_id_mask_uniform_slots`),
      spinner_draws: numberMetric(metrics, `${prefix}_spinner_draws`),
      camera_bg_draws: numberMetric(metrics, `${prefix}_camera_bg_draws`),
      render_passes: numberMetric(metrics, `${prefix}_render_passes`),
      clear_passes: numberMetric(metrics, `${prefix}_clear_passes`),
      draw_passes: numberMetric(metrics, `${prefix}_draw_passes`),
      scene3d_passes: numberMetric(metrics, `${prefix}_scene3d_passes`),
      scene3d_overlay_passes: numberMetric(metrics, `${prefix}_scene3d_overlay_passes`),
      id_mask_raster_passes: numberMetric(metrics, `${prefix}_id_mask_raster_passes`),
      id_mask_field_seed_passes: numberMetric(metrics, `${prefix}_id_mask_field_seed_passes`),
      id_mask_field_jump_passes: numberMetric(metrics, `${prefix}_id_mask_field_jump_passes`),
      id_mask_compositor_passes: numberMetric(metrics, `${prefix}_id_mask_compositor_passes`),
      present_passes: numberMetric(metrics, `${prefix}_present_passes`),
      texture_copies: numberMetric(metrics, `${prefix}_texture_copies`),
      command_buffers: numberMetric(metrics, `${prefix}_command_buffers`),
      ...timestampMetricFields(metrics, `${prefix}_`),
      buffer_upload_bytes: numberMetric(metrics, `${prefix}_buffer_upload_bytes`),
      texture_upload_bytes: numberMetric(metrics, `${prefix}_texture_upload_bytes`),
      ...resourceMetricFields(metrics, `${prefix}_`),
      image_upload_temp_allocs: numberMetric(metrics, `${prefix}_image_upload_temp_allocs`),
      image_upload_temp_bytes: numberMetric(metrics, `${prefix}_image_upload_temp_bytes`),
      image_upload_scratch_bytes: numberMetric(metrics, `${prefix}_image_upload_scratch_bytes`),
      image_upload_scratch_grows: numberMetric(metrics, `${prefix}_image_upload_scratch_grows`),
      ...scratchMetricFields(metrics, `${prefix}_`),
      vertices: numberMetric(metrics, "vertices"),
      vertex_bytes: numberMetric(metrics, "vertex_bytes"),
      unit: "ms/cpu-submit",
   };
}

function prefixedBackendCase(metrics, id, variant, prefix, extra)
{
   let samples = numberMetric(metrics, "samples");
   let framesPerSample = numberMetric(metrics, "frames_per_sample");
   return {
      id,
      layer: "engine",
      scenario: "browser-submit-throughput",
      variant,
      cache_state: "warm",
      refresh_mode: "unpaced-tight-loop",
      samples,
      frames_per_sample: framesPerSample,
      frames: samples * framesPerSample,
      p50_ms: numberMetric(metrics, `${prefix}_p50_ms`),
      p95_ms: numberMetric(metrics, `${prefix}_p95_ms`),
      p99_ms: numberMetric(metrics, `${prefix}_p99_ms`),
      peak_ms: numberMetric(metrics, `${prefix}_peak_ms`),
      avg_ms: numberMetric(metrics, `${prefix}_avg_ms`),
      ...allocationMetricFields(metrics, `${prefix}_`),
      ...submitAllocationMetricFields(metrics, `${prefix}_`),
      draws: numberMetric(metrics, `${prefix}_draws`),
      draw_items: numberMetric(metrics, `${prefix}_draw_items`),
      draw_items_coalesced: numberMetric(metrics, `${prefix}_draw_items_coalesced`),
      draw_pipeline_binds: numberMetric(metrics, `${prefix}_draw_pipeline_binds`),
      draw_bind_group_binds: numberMetric(metrics, `${prefix}_draw_bind_group_binds`),
      draw_scissor_sets: numberMetric(metrics, `${prefix}_draw_scissor_sets`),
      solid_tris: numberMetric(metrics, `${prefix}_solid_tris`),
      rrect_instances: numberMetric(metrics, `${prefix}_rrect_instances`),
      rrect_triangles: numberMetric(metrics, `${prefix}_rrect_triangles`),
      rrect_instance_bytes: numberMetric(metrics, `${prefix}_rrect_instance_bytes`),
      image_draws: numberMetric(metrics, `${prefix}_image_draws`),
      image_mesh_draws: numberMetric(metrics, `${prefix}_image_mesh_draws`),
      nine_slice_draws: numberMetric(metrics, `${prefix}_nine_slice_draws`),
      glyph_quads: numberMetric(metrics, `${prefix}_glyph_quads`),
      sdf_glyph_quads: numberMetric(metrics, `${prefix}_sdf_glyph_quads`),
      clip_depth_peak: numberMetric(metrics, `${prefix}_clip_depth_peak`),
      damage_rects: numberMetric(metrics, `${prefix}_damage_rects`),
      layer_draws: numberMetric(metrics, `${prefix}_layer_draws`),
      layer_cache_hits: numberMetric(metrics, `${prefix}_layer_cache_hits`),
      layer_cache_misses: numberMetric(metrics, `${prefix}_layer_cache_misses`),
      layer_cache_skipped_draws: numberMetric(metrics, `${prefix}_layer_cache_skipped_draws`),
      layer_passes: numberMetric(metrics, `${prefix}_layer_passes`),
      scene3d_draws: numberMetric(metrics, `${prefix}_scene3d_draws`),
      id_mask_draws: numberMetric(metrics, `${prefix}_id_mask_draws`),
      backdrop_draws: numberMetric(metrics, `${prefix}_backdrop_draws`),
      visual_effect_draws: numberMetric(metrics, `${prefix}_visual_effect_draws`),
      effect_uniform_writes: numberMetric(metrics, `${prefix}_effect_uniform_writes`),
      effect_uniform_bytes: numberMetric(metrics, `${prefix}_effect_uniform_bytes`),
      effect_uniform_slots: numberMetric(metrics, `${prefix}_effect_uniform_slots`),
      id_mask_uniform_writes: numberMetric(metrics, `${prefix}_id_mask_uniform_writes`),
      id_mask_uniform_bytes: numberMetric(metrics, `${prefix}_id_mask_uniform_bytes`),
      id_mask_uniform_slots: numberMetric(metrics, `${prefix}_id_mask_uniform_slots`),
      spinner_draws: numberMetric(metrics, `${prefix}_spinner_draws`),
      camera_bg_draws: numberMetric(metrics, `${prefix}_camera_bg_draws`),
      render_passes: numberMetric(metrics, `${prefix}_render_passes`),
      clear_passes: numberMetric(metrics, `${prefix}_clear_passes`),
      draw_passes: numberMetric(metrics, `${prefix}_draw_passes`),
      scene3d_passes: numberMetric(metrics, `${prefix}_scene3d_passes`),
      scene3d_overlay_passes: numberMetric(metrics, `${prefix}_scene3d_overlay_passes`),
      id_mask_raster_passes: numberMetric(metrics, `${prefix}_id_mask_raster_passes`),
      id_mask_field_seed_passes: numberMetric(metrics, `${prefix}_id_mask_field_seed_passes`),
      id_mask_field_jump_passes: numberMetric(metrics, `${prefix}_id_mask_field_jump_passes`),
      id_mask_compositor_passes: numberMetric(metrics, `${prefix}_id_mask_compositor_passes`),
      present_passes: numberMetric(metrics, `${prefix}_present_passes`),
      texture_copies: numberMetric(metrics, `${prefix}_texture_copies`),
      command_buffers: numberMetric(metrics, `${prefix}_command_buffers`),
      ...timestampMetricFields(metrics, `${prefix}_`),
      buffer_upload_bytes: numberMetric(metrics, `${prefix}_buffer_upload_bytes`),
      texture_upload_bytes: numberMetric(metrics, `${prefix}_texture_upload_bytes`),
      ...resourceMetricFields(metrics, `${prefix}_`),
      image_upload_temp_allocs: numberMetric(metrics, `${prefix}_image_upload_temp_allocs`),
      image_upload_temp_bytes: numberMetric(metrics, `${prefix}_image_upload_temp_bytes`),
      image_upload_scratch_bytes: numberMetric(metrics, `${prefix}_image_upload_scratch_bytes`),
      image_upload_scratch_grows: numberMetric(metrics, `${prefix}_image_upload_scratch_grows`),
      ...scratchMetricFields(metrics, `${prefix}_`),
      ...extra,
      unit: "ms/cpu-submit",
   };
}

const WARM_RESOURCE_CHURN_EXCLUDED_IDS = new Set([
   "web.wasm.webgpu.raf_frame_loop",
   "web.wasm.webgpu.scene3d.recreate_mesh",
   "web.wasm.webgpu.scene3d.stress_recreate_mesh",
]);

const WARM_RESOURCE_CHURN_FIELDS = [
   "buffer_grows",
   "texture_creates",
   "bind_group_creates",
   "pipeline_creates",
   "sampler_creates",
   "mesh3d_creates",
   "draw_buffer_grows",
   "image_texture_creates",
   "image_bind_group_creates",
   "target_texture_creates",
   "target_bind_group_creates",
   "layer_texture_creates",
   "layer_bind_group_creates",
   "scene3d_buffer_grows",
   "scene3d_bind_group_creates",
   "effect_buffer_grows",
   "effect_bind_group_creates",
   "id_mask_texture_creates",
   "id_mask_buffer_grows",
   "id_mask_bind_group_creates",
   "image_upload_temp_allocs",
   "image_upload_temp_bytes",
   "image_upload_scratch_grows",
   "cpu_scratch_grows",
   "cpu_scratch_growth_bytes",
   "cpu_draw_scratch_grows",
   "cpu_draw_scratch_growth_bytes",
   "cpu_scene3d_scratch_grows",
   "cpu_scene3d_scratch_growth_bytes",
   "cpu_effect_scratch_grows",
   "cpu_effect_scratch_growth_bytes",
   "cpu_id_mask_scratch_grows",
   "cpu_id_mask_scratch_growth_bytes",
   "cpu_image_upload_scratch_grows",
   "cpu_image_upload_scratch_growth_bytes",
   "cpu_resource_table_scratch_grows",
   "cpu_resource_table_scratch_growth_bytes",
];

const GPU_TIMESTAMP_STAGE_FIELDS = [
   ["clear", "clear_passes", "gpu_timestamp_clear_ns"],
   ["draw", "draw_passes", "gpu_timestamp_draw_ns"],
   ["scene3d", "scene3d_passes", "gpu_timestamp_scene3d_ns"],
   ["scene3d_overlay", "scene3d_overlay_passes", "gpu_timestamp_scene3d_overlay_ns"],
   ["id_mask_raster", "id_mask_raster_passes", "gpu_timestamp_id_mask_raster_ns"],
   ["id_mask_field_seed", "id_mask_field_seed_passes", "gpu_timestamp_id_mask_field_seed_ns"],
   ["id_mask_field_jump", "id_mask_field_jump_passes", "gpu_timestamp_id_mask_field_jump_ns"],
   ["id_mask_compositor", "id_mask_compositor_passes", "gpu_timestamp_id_mask_compositor_ns"],
   ["present", "present_passes", "gpu_timestamp_present_ns"],
];

const EXPECTED_BENCHMARK_MARKS = [
   "cpu_submit_throughput",
   "raf_frame_loop",
   "id_mask_current",
   "upload_current",
   "effect_uniform_ab",
   "backdrop_batch_current",
   "scene3d_ab",
   "mixed_matrix",
   "layer_effects_matrix",
   "clean_layer_ab",
   "command_family_matrix",
   "glyph_run_current",
   "neon_marker_ab",
   "direct_surface_ab",
];

const WEBGPU_BACKEND_PATHS = [
   {
      id: "cpu_submit_throughput",
      rows: ["web.wasm.webgpu.cpu_submit_throughput"],
      counters: ["draws", "draw_items", "draw_passes", "command_buffers", "buffer_upload_bytes", "gpu_timestamp_passes"],
      comparison: "coverage",
   },
   {
      id: "raf_frame_loop",
      rows: ["web.wasm.webgpu.raf_frame_loop"],
      counters: ["frames", "submissions", "p50_ms", "p95_ms", "p99_ms", "peak_ms"],
      comparison: "coverage",
   },
   {
      id: "id_mask_compositor",
      rows: ["web.wasm.webgpu.id_mask_compositor.current"],
      counters: ["id_mask_draws", "id_mask_uniform_writes", "id_mask_uniform_bytes", "id_mask_uniform_slots", "id_mask_raster_passes", "id_mask_field_seed_passes", "id_mask_field_jump_passes", "id_mask_compositor_passes", "buffer_upload_bytes", "vertices", "gpu_timestamp_passes"],
      comparison: "coverage",
   },
   {
      id: "clean_layer_reuse",
      rows: ["web.wasm.webgpu.clean_layer.clean_reuse"],
      counters: ["layer_draws", "layer_cache_hits", "layer_cache_misses", "layer_cache_skipped_draws", "layer_passes", "draw_items", "draw_passes", "gpu_timestamp_passes"],
      comparison: "current",
   },
   {
      id: "glyph_atlas_upload",
      rows: ["web.wasm.webgpu.glyph_atlas_upload.current_dirty"],
      counters: ["glyph_quads", "texture_upload_bytes", "buffer_upload_bytes", "gpu_timestamp_passes"],
      comparison: "coverage",
   },
   {
      id: "image_upload",
      rows: ["web.wasm.webgpu.image_upload.current_dirty"],
      counters: ["image_draws", "texture_upload_bytes", "buffer_upload_bytes", "gpu_timestamp_passes"],
      comparison: "coverage",
   },
   {
      id: "effect_uniform",
      rows: ["web.wasm.webgpu.effect_uniform.current_batched"],
      counters: ["backdrop_draws", "visual_effect_draws", "effect_uniform_writes", "effect_uniform_slots", "texture_copies", "render_passes", "gpu_timestamp_total_ns"],
      comparison: "current",
   },
   {
      id: "backdrop_batch",
      rows: ["web.wasm.webgpu.backdrop_batch.current_coalesced"],
      counters: ["backdrop_draws", "effect_uniform_slots", "texture_copies", "render_passes", "gpu_timestamp_passes"],
      comparison: "current",
   },
   {
      id: "scene3d_mesh_reuse",
      rows: ["web.wasm.webgpu.scene3d.reused_mesh", "web.wasm.webgpu.scene3d.recreate_mesh"],
      counters: ["scene3d_draws", "mesh3d_creates", "buffer_grows", "cpu_scratch_grows", "gpu_timestamp_passes"],
      comparison: "current_vs_legacy",
   },
   {
      id: "scene3d_stress_mesh_reuse",
      rows: ["web.wasm.webgpu.scene3d.stress_reused_mesh", "web.wasm.webgpu.scene3d.stress_recreate_mesh"],
      counters: ["scene3d_draws", "mesh3d_creates", "buffer_grows", "cpu_scratch_grows", "gpu_timestamp_passes"],
      comparison: "current_vs_legacy",
   },
   {
      id: "mixed_text_image_effects",
      rows: ["web.wasm.webgpu.mixed_text_image_effects"],
      counters: ["glyph_quads", "image_draws", "image_tiles", "backdrop_draws", "visual_effect_draws", "spinner_draws", "layer_draws", "damage_rects", "draw_pipeline_binds", "draw_bind_group_binds", "draw_scissor_sets", "effect_uniform_writes", "texture_copies", "render_passes", "gpu_timestamp_passes"],
      comparison: "current",
   },
   {
      id: "layer_damage_effects",
      rows: ["web.wasm.webgpu.layer_damage_effects"],
      counters: ["glyph_quads", "image_draws", "image_tiles", "layer_draws", "damage_rects", "clip_depth_peak", "backdrop_draws", "visual_effect_draws", "spinner_draws", "draw_pipeline_binds", "draw_bind_group_binds", "draw_scissor_sets", "effect_uniform_writes", "texture_copies", "render_passes", "gpu_timestamp_passes"],
      comparison: "current",
   },
   {
      id: "command_family_matrix",
      rows: ["web.wasm.webgpu.command_family_matrix"],
      counters: ["image_mesh_draws", "nine_slice_draws", "sdf_glyph_quads", "camera_bg_draws", "expected_camera_bg", "draw_items", "draw_pipeline_binds", "draw_bind_group_binds", "draw_scissor_sets", "gpu_timestamp_passes"],
      comparison: "current",
   },
   {
      id: "glyph_run",
      rows: ["web.wasm.webgpu.glyph_run.current"],
      counters: ["expected_glyph_runs", "expected_glyph_quads", "expected_sdf_glyph_quads", "expected_draw_items", "draw_items", "glyph_quads", "sdf_glyph_quads", "draw_pipeline_binds", "draw_bind_group_binds", "draw_scissor_sets", "render_passes", "gpu_timestamp_passes"],
      comparison: "current",
   },
   {
      id: "neon_marker",
      rows: ["web.wasm.webgpu.neon_marker.current"],
      counters: ["expected_markers", "expected_draw_items", "draw_items", "solid_tris", "draw_pipeline_binds", "draw_bind_group_binds", "draw_scissor_sets", "gpu_timestamp_passes"],
      comparison: "current",
   },
   {
      id: "direct_surface",
      rows: ["web.wasm.webgpu.direct_surface.current"],
      counters: ["expected_draw_items", "expected_image_draws", "draw_items", "image_draws", "render_passes", "draw_passes", "clear_passes", "present_passes", "texture_copies", "gpu_timestamp_passes"],
      comparison: "current",
   },
];

function benchmarkMarkSummary(pageReport, traceSummary)
{
   let marks = Array.isArray(pageReport.benchmark_marks) ? pageReport.benchmark_marks : [];
   let normalized = [];
   let byId = new Map();
   for (let mark of marks) {
      if (!mark || typeof mark.id !== "string") {
         throw new Error("web report missing benchmark mark id");
      }
      let durationMs = Number(mark.duration_ms);
      let startMs = Number(mark.start_ms);
      if (!Number.isFinite(durationMs) || durationMs <= 0.0) {
         throw new Error(`web report benchmark mark ${mark.id} missing positive duration`);
      }
      if (!Number.isFinite(startMs) || startMs < 0.0) {
         throw new Error(`web report benchmark mark ${mark.id} missing finite start`);
      }
      let wasmBeforeBytes = Number(mark.wasm_memory_before_bytes);
      let wasmAfterBytes = Number(mark.wasm_memory_after_bytes);
      let wasmGrowthBytes = Number(mark.wasm_memory_growth_bytes);
      if (!Number.isFinite(wasmBeforeBytes) || wasmBeforeBytes <= 0.0) {
         throw new Error(`web report benchmark mark ${mark.id} missing wasm memory before bytes`);
      }
      if (!Number.isFinite(wasmAfterBytes) || wasmAfterBytes < wasmBeforeBytes) {
         throw new Error(`web report benchmark mark ${mark.id} missing wasm memory after bytes`);
      }
      if (!Number.isFinite(wasmGrowthBytes) || wasmGrowthBytes < 0.0) {
         throw new Error(`web report benchmark mark ${mark.id} missing wasm memory growth bytes`);
      }
      let jsHeapSampleSupported = Number(mark.js_heap_sample_supported);
      let jsHeapGcAvailable = Number(mark.js_heap_gc_available);
      let jsHeapBeforeBytes = Number(mark.js_heap_before_bytes);
      let jsHeapAfterBytes = Number(mark.js_heap_after_bytes);
      let jsHeapGrowthBytes = Number(mark.js_heap_growth_bytes);
      if (!Number.isFinite(jsHeapSampleSupported) || jsHeapSampleSupported < 0.0) {
         throw new Error(`web report benchmark mark ${mark.id} missing JS heap sample support`);
      }
      if (!Number.isFinite(jsHeapGcAvailable) || jsHeapGcAvailable < 0.0) {
         throw new Error(`web report benchmark mark ${mark.id} missing JS heap GC support`);
      }
      if (!Number.isFinite(jsHeapBeforeBytes) || jsHeapBeforeBytes < 0.0) {
         throw new Error(`web report benchmark mark ${mark.id} missing JS heap before bytes`);
      }
      if (!Number.isFinite(jsHeapAfterBytes) || jsHeapAfterBytes < 0.0) {
         throw new Error(`web report benchmark mark ${mark.id} missing JS heap after bytes`);
      }
      if (!Number.isFinite(jsHeapGrowthBytes) || jsHeapGrowthBytes < 0.0) {
         throw new Error(`web report benchmark mark ${mark.id} missing JS heap growth bytes`);
      }
      let entry = {
         id: mark.id,
         name: typeof mark.name === "string" ? mark.name : "",
         start_ms: startMs,
         duration_ms: durationMs,
         wasm_memory_before_bytes: wasmBeforeBytes,
         wasm_memory_after_bytes: wasmAfterBytes,
         wasm_memory_growth_bytes: wasmGrowthBytes,
         js_heap_sample_supported: jsHeapSampleSupported,
         js_heap_gc_available: jsHeapGcAvailable,
         js_heap_before_bytes: jsHeapBeforeBytes,
         js_heap_after_bytes: jsHeapAfterBytes,
         js_heap_growth_bytes: jsHeapGrowthBytes,
      };
      normalized.push(entry);
      byId.set(mark.id, entry);
   }
   let missing = [];
   for (let id of EXPECTED_BENCHMARK_MARKS) {
      if (!byId.has(id)) {
         missing.push(id);
      }
   }
   if (missing.length > 0) {
      throw new Error(`web report missing benchmark marks: ${missing.join(", ")}`);
   }
   let traceLabels = Array.isArray(traceSummary?.benchmark_trace_mark_labels)
      ? traceSummary.benchmark_trace_mark_labels
      : [];
   let traced = EXPECTED_BENCHMARK_MARKS.filter(id => traceLabels.includes(id));
   let growthMarks = normalized.filter(mark => mark.wasm_memory_growth_bytes > 0.0);
   let jsHeapGrowthMarks = normalized.filter(mark => mark.js_heap_growth_bytes > 0.0);
   return {
      id: "web.wasm.webgpu.benchmark_mark_coverage",
      expected_count: EXPECTED_BENCHMARK_MARKS.length,
      page_mark_count: normalized.length,
      traced_mark_count: traced.length,
      js_heap_sample_supported_count: normalized.filter(mark => mark.js_heap_sample_supported > 0.0).length,
      js_heap_gc_available_count: normalized.filter(mark => mark.js_heap_gc_available > 0.0).length,
      js_heap_total_growth_bytes: normalized.reduce(
         (sum, mark) => sum + mark.js_heap_growth_bytes,
         0,
      ),
      js_heap_max_growth_bytes: normalized.reduce(
         (max, mark) => Math.max(max, mark.js_heap_growth_bytes),
         0,
      ),
      js_heap_growth_labels: jsHeapGrowthMarks.map(mark => mark.id),
      wasm_memory_total_growth_bytes: normalized.reduce(
         (sum, mark) => sum + mark.wasm_memory_growth_bytes,
         0,
      ),
      wasm_memory_max_growth_bytes: normalized.reduce(
         (max, mark) => Math.max(max, mark.wasm_memory_growth_bytes),
         0,
      ),
      wasm_memory_growth_labels: growthMarks.map(mark => mark.id),
      expected: [...EXPECTED_BENCHMARK_MARKS],
      page_labels: normalized.map(mark => mark.id),
      traced_labels: traced,
      marks: normalized,
   };
}

function warmResourceChurnSummary(cases)
{
   let totals = {};
   for (let field of WARM_RESOURCE_CHURN_FIELDS) {
      totals[`total_${field}`] = 0;
   }
   let rows = [];
   let rowDetails = [];
   let excluded = [];
   for (let row of cases) {
      if (WARM_RESOURCE_CHURN_EXCLUDED_IDS.has(row.id)) {
         excluded.push(row.id);
         continue;
      }
      rows.push(row.id);
      let detail = { id: row.id };
      for (let field of WARM_RESOURCE_CHURN_FIELDS) {
         let value = row[field];
         if (!Number.isFinite(value)) {
            throw new Error(`web report missing finite warm resource churn field ${row.id}.${field}`);
         }
         totals[`total_${field}`] += value;
         detail[field] = value;
      }
      rowDetails.push(detail);
   }
   return {
      id: "web.wasm.webgpu.warm_resource_churn.current_rows",
      checked_rows: rows.length,
      excluded_rows: excluded.length,
      row_detail_count: rowDetails.length,
      zero_growth_fields: [...WARM_RESOURCE_CHURN_FIELDS],
      rows,
      excluded,
      row_details: rowDetails,
      ...totals,
   };
}

function gpuTimestampStageBreakdownSummary(cases)
{
   let stages = GPU_TIMESTAMP_STAGE_FIELDS.map(([stage, passField, timestampField]) => ({
      stage,
      pass_field: passField,
      timestamp_field: timestampField,
      pass_count: 0,
      timestamp_ns: 0,
   }));
   let rowDetails = [];
   let totalRenderPasses = 0;
   let totalTimestampPasses = 0;
   let totalTimestampNs = 0;
   let totalFamilyPasses = 0;
   let totalFamilyTimestampNs = 0;
   for (let row of cases) {
      if (row.id === "web.wasm.webgpu.raf_frame_loop") {
         continue;
      }
      let familyPasses = 0;
      let familyTimestampNs = 0;
      let detailStages = [];
      for (let index = 0; index < GPU_TIMESTAMP_STAGE_FIELDS.length; index++) {
         let [stage, passField, timestampField] = GPU_TIMESTAMP_STAGE_FIELDS[index];
         let passCount = row[passField];
         let timestampNs = row[timestampField];
         if (!Number.isFinite(passCount) || !Number.isFinite(timestampNs)) {
            throw new Error(`web report missing finite GPU timestamp stage field ${row.id}.${stage}`);
         }
         familyPasses += passCount;
         familyTimestampNs += timestampNs;
         stages[index].pass_count += passCount;
         stages[index].timestamp_ns += timestampNs;
         detailStages.push({
            stage,
            pass_count: passCount,
            timestamp_ns: timestampNs,
         });
      }
      totalRenderPasses += row.render_passes;
      totalTimestampPasses += row.gpu_timestamp_passes;
      totalTimestampNs += row.gpu_timestamp_total_ns;
      totalFamilyPasses += familyPasses;
      totalFamilyTimestampNs += familyTimestampNs;
      rowDetails.push({
         id: row.id,
         render_passes: row.render_passes,
         gpu_timestamp_passes: row.gpu_timestamp_passes,
         gpu_timestamp_total_ns: row.gpu_timestamp_total_ns,
         family_passes: familyPasses,
         family_timestamp_ns: familyTimestampNs,
         stages: detailStages,
      });
   }
   return {
      id: "web.wasm.webgpu.gpu_timestamp_stage_breakdown",
      row_count: rowDetails.length,
      collected_rows: cases.filter(row => row.gpu_timestamp_passes > 0).length,
      stage_count: stages.length,
      row_detail_count: rowDetails.length,
      total_render_passes: totalRenderPasses,
      total_timestamp_passes: totalTimestampPasses,
      total_timestamp_ns: totalTimestampNs,
      total_family_passes: totalFamilyPasses,
      total_family_timestamp_ns: totalFamilyTimestampNs,
      stages,
      row_details: rowDetails,
   };
}

function wasmAllocationSummary(cases)
{
   let rows = [];
   let rowDetails = [];
   let excluded = [];
   let totalAllocCount = 0;
   let totalAllocBytes = 0;
   let totalReallocCount = 0;
   let totalReallocGrowBytes = 0;
   let maxAllocsPerFrame = 0;
   let maxAllocBytesPerFrame = 0;
   let maxPeakFrameAllocBytes = 0;
   for (let row of cases) {
      if (WARM_RESOURCE_CHURN_EXCLUDED_IDS.has(row.id)) {
         excluded.push(row.id);
         continue;
      }
      let frames = Number(row.frames);
      let allocCount = Number(row.wasm_alloc_count);
      let allocBytes = Number(row.wasm_alloc_bytes);
      let reallocCount = Number(row.wasm_realloc_count);
      let reallocGrowBytes = Number(row.wasm_realloc_grow_bytes);
      let allocatingFrames = Number(row.wasm_allocating_frames);
      let peakFrameAllocBytes = Number(row.wasm_peak_frame_alloc_bytes);
      let allocsPerFrame = frames > 0 ? allocCount / frames : 0;
      let allocBytesPerFrame = frames > 0 ? allocBytes / frames : 0;
      rows.push(row.id);
      totalAllocCount += allocCount;
      totalAllocBytes += allocBytes;
      totalReallocCount += reallocCount;
      totalReallocGrowBytes += reallocGrowBytes;
      maxAllocsPerFrame = Math.max(maxAllocsPerFrame, allocsPerFrame);
      maxAllocBytesPerFrame = Math.max(maxAllocBytesPerFrame, allocBytesPerFrame);
      maxPeakFrameAllocBytes = Math.max(maxPeakFrameAllocBytes, peakFrameAllocBytes);
      rowDetails.push({
         id: row.id,
         frames,
         wasm_alloc_count: allocCount,
         wasm_alloc_bytes: allocBytes,
         wasm_allocs_per_frame: allocsPerFrame,
         wasm_alloc_bytes_per_frame: allocBytesPerFrame,
         wasm_dealloc_count: numberMetric(row, "wasm_dealloc_count"),
         wasm_dealloc_bytes: numberMetric(row, "wasm_dealloc_bytes"),
         wasm_realloc_count: reallocCount,
         wasm_realloc_grow_bytes: reallocGrowBytes,
         wasm_realloc_shrink_bytes: numberMetric(row, "wasm_realloc_shrink_bytes"),
         wasm_allocating_frames: allocatingFrames,
         wasm_peak_frame_alloc_bytes: peakFrameAllocBytes,
      });
   }
   return {
      id: "web.wasm.webgpu.wasm_allocation_audit.current_rows",
      status: "measured",
      checked_count: rows.length,
      excluded_count: excluded.length,
      total_wasm_alloc_count: totalAllocCount,
      total_wasm_alloc_bytes: totalAllocBytes,
      total_wasm_realloc_count: totalReallocCount,
      total_wasm_realloc_grow_bytes: totalReallocGrowBytes,
      max_wasm_allocs_per_frame: maxAllocsPerFrame,
      max_wasm_alloc_bytes_per_frame: maxAllocBytesPerFrame,
      max_wasm_peak_frame_alloc_bytes: maxPeakFrameAllocBytes,
      budget_wasm_allocs_per_frame: 7,
      budget_wasm_alloc_bytes_per_frame: 144,
      rows,
      excluded,
      row_detail_count: rowDetails.length,
      row_details: rowDetails,
   };
}

function wasmAllocationInvarianceSummary(allocationSummary)
{
   let signatures = new Map();
   for (let row of allocationSummary.row_details) {
      let signature = [
         row.frames,
         row.wasm_alloc_count,
         row.wasm_alloc_bytes,
         row.wasm_dealloc_count,
         row.wasm_dealloc_bytes,
         row.wasm_realloc_count,
         row.wasm_realloc_grow_bytes,
         row.wasm_realloc_shrink_bytes,
         row.wasm_allocating_frames,
         row.wasm_peak_frame_alloc_bytes,
      ].join(":");
      let ids = signatures.get(signature);
      if (ids) {
         ids.push(row.id);
      } else {
         signatures.set(signature, [row.id]);
      }
   }
   let signatureRows = [...signatures.entries()].map(([signature, ids]) => ({ signature, ids }));
   let reference = allocationSummary.row_details.find(row => row.id === "web.wasm.webgpu.cpu_submit_throughput");
   return {
      id: "web.wasm.webgpu.wasm_allocation_invariance.current_rows",
      status: signatures.size === 1 ? "shared-submit-boundary-profile" : "path-specific-allocations",
      reference_row: reference ? reference.id : "",
      checked_count: allocationSummary.checked_count,
      unique_signature_count: signatures.size,
      shared_wasm_alloc_count: reference ? reference.wasm_alloc_count : 0,
      shared_wasm_alloc_bytes: reference ? reference.wasm_alloc_bytes : 0,
      shared_wasm_realloc_count: reference ? reference.wasm_realloc_count : 0,
      shared_wasm_realloc_grow_bytes: reference ? reference.wasm_realloc_grow_bytes : 0,
      shared_wasm_allocating_frames: reference ? reference.wasm_allocating_frames : 0,
      shared_wasm_peak_frame_alloc_bytes: reference ? reference.wasm_peak_frame_alloc_bytes : 0,
      signature_rows: signatureRows,
   };
}

function frameLoopWasmStageSummary(cases)
{
   let frame = cases.find(row => row.id === "web.wasm.webgpu.cpu_submit_throughput");
   if (!frame) {
      throw new Error("web report missing frame-loop row for WASM stage allocation summary");
   }
   let totalAllocCount = 0;
   let totalAllocBytes = 0;
   let totalReallocCount = 0;
   let totalReallocGrowBytes = 0;
   let maxPeakFrameAllocBytes = 0;
   let dominantStage = "";
   let dominantStageAllocCount = 0;
   let stages = [];
   for (let name of WASM_FRAME_STAGE_NAMES) {
      let prefix = `wasm_stage_${name}_`;
      let allocCount = numberMetric(frame, `${prefix}alloc_count`);
      let allocBytes = numberMetric(frame, `${prefix}alloc_bytes`);
      let reallocCount = numberMetric(frame, `${prefix}realloc_count`);
      let reallocGrowBytes = numberMetric(frame, `${prefix}realloc_grow_bytes`);
      let peakFrameAllocBytes = numberMetric(frame, `${prefix}peak_frame_alloc_bytes`);
      totalAllocCount += allocCount;
      totalAllocBytes += allocBytes;
      totalReallocCount += reallocCount;
      totalReallocGrowBytes += reallocGrowBytes;
      maxPeakFrameAllocBytes = Math.max(maxPeakFrameAllocBytes, peakFrameAllocBytes);
      if (allocCount > dominantStageAllocCount) {
         dominantStage = name;
         dominantStageAllocCount = allocCount;
      }
      stages.push({
         stage: name,
         wasm_alloc_count: allocCount,
         wasm_alloc_bytes: allocBytes,
         wasm_realloc_count: reallocCount,
         wasm_realloc_grow_bytes: reallocGrowBytes,
         wasm_peak_frame_alloc_bytes: peakFrameAllocBytes,
      });
   }
   return {
      id: "web.wasm.webgpu.frame_loop_wasm_allocation_stages",
      row_id: frame.id,
      frames: numberMetric(frame, "frames"),
      stage_count: stages.length,
      total_stage_wasm_alloc_count: totalAllocCount,
      total_stage_wasm_alloc_bytes: totalAllocBytes,
      total_stage_wasm_realloc_count: totalReallocCount,
      total_stage_wasm_realloc_grow_bytes: totalReallocGrowBytes,
      max_stage_peak_frame_alloc_bytes: maxPeakFrameAllocBytes,
      row_wasm_alloc_count: numberMetric(frame, "wasm_alloc_count"),
      row_wasm_alloc_bytes: numberMetric(frame, "wasm_alloc_bytes"),
      row_wasm_realloc_count: numberMetric(frame, "wasm_realloc_count"),
      dominant_stage: dominantStage,
      dominant_stage_wasm_alloc_count: dominantStageAllocCount,
      stages,
   };
}

function frameLoopWasmSubmitStageSummary(cases)
{
   let frame = cases.find(row => row.id === "web.wasm.webgpu.cpu_submit_throughput");
   if (!frame) {
      throw new Error("web report missing frame-loop row for WASM submit allocation summary");
   }
   let stages = [];
   let totalAllocCount = 0;
   let totalAllocBytes = 0;
   let dominantStage = "";
   let dominantStageAllocCount = 0;
   for (let name of WASM_SUBMIT_STAGE_NAMES) {
      let key = `submit_${name}_`;
      let allocCount = numberMetric(frame, `${key}alloc_count`);
      let allocBytes = numberMetric(frame, `${key}alloc_bytes`);
      totalAllocCount += allocCount;
      totalAllocBytes += allocBytes;
      if (allocCount > dominantStageAllocCount) {
         dominantStage = name;
         dominantStageAllocCount = allocCount;
      }
      stages.push({
         stage: name,
         wasm_alloc_count: allocCount,
         wasm_alloc_bytes: allocBytes,
      });
   }
   return {
      id: "web.wasm.webgpu.frame_loop_wasm_submit_allocation_stages",
      row_id: frame.id,
      frames: numberMetric(frame, "frames"),
      stage_count: stages.length,
      total_stage_wasm_alloc_count: totalAllocCount,
      total_stage_wasm_alloc_bytes: totalAllocBytes,
      row_submit_wasm_alloc_count: numberMetric(frame, "submit_total_alloc_count"),
      row_submit_wasm_alloc_bytes: numberMetric(frame, "submit_total_alloc_bytes"),
      row_submit_wasm_realloc_count: numberMetric(frame, "submit_total_realloc_count"),
      row_submit_wasm_realloc_grow_bytes: numberMetric(
         frame,
         "submit_total_realloc_grow_bytes",
      ),
      frame_stage_submit_wasm_alloc_count: numberMetric(frame, "wasm_stage_submit_alloc_count"),
      frame_stage_submit_wasm_alloc_bytes: numberMetric(frame, "wasm_stage_submit_alloc_bytes"),
      dominant_stage: dominantStage,
      dominant_stage_wasm_alloc_count: dominantStageAllocCount,
      stages,
   };
}

function backendPathCoverageSummary(cases)
{
   let byId = new Map(cases.map(row => [row.id, row]));
   let paths = [];
   for (let spec of WEBGPU_BACKEND_PATHS) {
      let rowDetails = [];
      let missingRows = [];
      let missingCounters = [];
      for (let rowId of spec.rows) {
         let row = byId.get(rowId);
         if (!row) {
            missingRows.push(rowId);
            continue;
         }
         let counters = {};
         for (let field of spec.counters) {
            let value = row[field];
            if (!Number.isFinite(value)) {
               missingCounters.push(`${rowId}.${field}`);
               continue;
            }
            counters[field] = value;
         }
         let p50Ms = row.p50_ms ?? row.cpu_submit_p50_ms;
         let p95Ms = row.p95_ms ?? row.cpu_submit_p95_ms;
         let p99Ms = row.p99_ms ?? row.cpu_submit_p99_ms;
         let peakMs = row.peak_ms ?? row.cpu_submit_peak_ms;
         rowDetails.push({
            id: rowId,
            p50_ms: p50Ms,
            p95_ms: p95Ms,
            p99_ms: p99Ms,
            peak_ms: peakMs,
            counters,
         });
      }
      paths.push({
         id: spec.id,
         status: missingRows.length === 0 && missingCounters.length === 0 ? "covered" : "missing",
         comparison: spec.comparison,
         row_count: spec.rows.length,
         rows: [...spec.rows],
         counter_count: spec.counters.length,
         counters: [...spec.counters],
         missing_rows: missingRows,
         missing_counters: missingCounters,
         row_details: rowDetails,
      });
   }
   let covered = paths.filter(path => path.status === "covered");
   return {
      id: "web.wasm.webgpu.backend_path_coverage",
      expected_path_count: WEBGPU_BACKEND_PATHS.length,
      covered_path_count: covered.length,
      missing_path_count: paths.length - covered.length,
      paths,
   };
}

function buildWebReport(args, url, pageReport, pixelReport, traceSummary)
{
   if (pageReport?.benchmark_error) {
      let error = pageReport.benchmark_error;
      let id = typeof error.id === "string" ? error.id : "unknown";
      let detail = typeof error.detail === "string" ? error.detail : "unknown";
      throw new Error(`web benchmark report failed during ${id}: ${detail}`);
   }
   let frameMetrics = parseMetricString(pageReport.frame_perf);
   let idMaskMetrics = parseMetricString(pageReport.id_mask_current);
   let uploadMetrics = parseMetricString(pageReport.upload_current);
   let effectUniformMetrics = parseMetricString(pageReport.effect_uniform_ab);
   let backdropBatchMetrics = parseMetricString(pageReport.backdrop_batch_current);
   let scene3dMetrics = parseMetricString(pageReport.scene3d_ab);
   let mixedMetrics = parseMetricString(pageReport.mixed_matrix);
   let layerEffectsMetrics = parseMetricString(pageReport.layer_effects_matrix);
   let cleanLayerMetrics = parseMetricString(pageReport.clean_layer_ab);
   let commandFamilyMetrics = parseMetricString(pageReport.command_family_matrix);
   let glyphRunMetrics = parseMetricString(pageReport.glyph_run_current);
   let neonMarkerMetrics = parseMetricString(pageReport.neon_marker_ab);
   let directSurfaceMetrics = parseMetricString(pageReport.direct_surface_ab);
   let timingMetrics = parseMetricString(pageReport.webgpu_timing);
   let cases = [
      cpuSubmitCase(frameMetrics),
      rafFrameCase(pageReport.raf_frame_perf),
      idMaskCase(
         idMaskMetrics,
         "web.wasm.webgpu.id_mask_compositor.current",
         "webgpu-current",
         "current",
      ),
      prefixedBackendCase(
         uploadMetrics,
         "web.wasm.webgpu.glyph_atlas_upload.current_dirty",
         "webgpu-dirty-atlas-update",
         "glyph_current",
         {
            atlas_width: numberMetric(uploadMetrics, "atlas_width"),
            atlas_height: numberMetric(uploadMetrics, "atlas_height"),
            dirty_width: numberMetric(uploadMetrics, "atlas_dirty_width"),
            dirty_height: numberMetric(uploadMetrics, "atlas_dirty_height"),
         },
      ),
      prefixedBackendCase(
         uploadMetrics,
         "web.wasm.webgpu.image_upload.current_dirty",
         "webgpu-dirty-rgba-update",
         "image_current",
         {
            image_width: numberMetric(uploadMetrics, "image_width"),
            image_height: numberMetric(uploadMetrics, "image_height"),
            dirty_width: numberMetric(uploadMetrics, "image_dirty_width"),
            dirty_height: numberMetric(uploadMetrics, "image_dirty_height"),
         },
      ),
      prefixedBackendCase(
         effectUniformMetrics,
         "web.wasm.webgpu.effect_uniform.current_batched",
         "webgpu-effect-uniform-current-batched",
         "current",
         {
            expected_backdrops: numberMetric(effectUniformMetrics, "expected_backdrops"),
            sigma: numberMetric(effectUniformMetrics, "sigma"),
         },
      ),
      prefixedBackendCase(
         backdropBatchMetrics,
         "web.wasm.webgpu.backdrop_batch.current_coalesced",
         "webgpu-backdrop-batch-current-coalesced",
         "current",
         {
            expected_backdrops: numberMetric(backdropBatchMetrics, "expected_backdrops"),
            sigma: numberMetric(backdropBatchMetrics, "sigma"),
         },
      ),
      prefixedBackendCase(
         scene3dMetrics,
         "web.wasm.webgpu.scene3d.reused_mesh",
         "webgpu-scene3d-reused-mesh",
         "reused",
         {
            meshes: numberMetric(scene3dMetrics, "meshes"),
            instances: numberMetric(scene3dMetrics, "instances"),
         },
      ),
      prefixedBackendCase(
         scene3dMetrics,
         "web.wasm.webgpu.scene3d.recreate_mesh",
         "webgpu-scene3d-recreate-mesh",
         "recreate",
         {
            meshes: numberMetric(scene3dMetrics, "meshes"),
            instances: numberMetric(scene3dMetrics, "instances"),
         },
      ),
      prefixedBackendCase(
         scene3dMetrics,
         "web.wasm.webgpu.scene3d.stress_reused_mesh",
         "webgpu-scene3d-stress-reused-mesh",
         "stress_reused",
         {
            meshes: numberMetric(scene3dMetrics, "stress_meshes"),
            instances: numberMetric(scene3dMetrics, "stress_instances"),
         },
      ),
      prefixedBackendCase(
         scene3dMetrics,
         "web.wasm.webgpu.scene3d.stress_recreate_mesh",
         "webgpu-scene3d-stress-recreate-mesh",
         "stress_recreate",
         {
            meshes: numberMetric(scene3dMetrics, "stress_meshes"),
            instances: numberMetric(scene3dMetrics, "stress_instances"),
         },
      ),
      prefixedBackendCase(
         mixedMetrics,
         "web.wasm.webgpu.mixed_text_image_effects",
         "webgpu-mixed-effects-current",
         "current",
         {
            glyphs: numberMetric(mixedMetrics, "glyphs"),
            image_tiles: numberMetric(mixedMetrics, "image_tiles"),
            image_width: numberMetric(mixedMetrics, "image_width"),
            image_height: numberMetric(mixedMetrics, "image_height"),
         },
      ),
      prefixedBackendCase(
         layerEffectsMetrics,
         "web.wasm.webgpu.layer_damage_effects",
         "webgpu-layer-damage-effects-current",
         "current",
         {
            glyphs: numberMetric(layerEffectsMetrics, "glyphs"),
            image_tiles: numberMetric(layerEffectsMetrics, "image_tiles"),
            image_width: numberMetric(layerEffectsMetrics, "image_width"),
            image_height: numberMetric(layerEffectsMetrics, "image_height"),
            expected_layers: numberMetric(layerEffectsMetrics, "expected_layers"),
            expected_damage_rects: numberMetric(layerEffectsMetrics, "expected_damage_rects"),
            expected_backdrops: numberMetric(layerEffectsMetrics, "expected_backdrops"),
         },
      ),
      prefixedBackendCase(
         cleanLayerMetrics,
         "web.wasm.webgpu.clean_layer.clean_reuse",
         "webgpu-clean-layer-clean-reuse",
         "clean",
         {
            glyphs: numberMetric(cleanLayerMetrics, "glyphs"),
            image_tiles: numberMetric(cleanLayerMetrics, "image_tiles"),
            image_width: numberMetric(cleanLayerMetrics, "image_width"),
            image_height: numberMetric(cleanLayerMetrics, "image_height"),
            expected_layers: numberMetric(cleanLayerMetrics, "expected_layers"),
            expected_clean_hits: numberMetric(cleanLayerMetrics, "expected_clean_hits"),
         },
      ),
      prefixedBackendCase(
         commandFamilyMetrics,
         "web.wasm.webgpu.command_family_matrix",
         "webgpu-command-family-current",
         "current",
         {
            expected_image_meshes: numberMetric(commandFamilyMetrics, "expected_image_meshes"),
            expected_nine_slices: numberMetric(commandFamilyMetrics, "expected_nine_slices"),
            expected_sdf_glyphs: numberMetric(commandFamilyMetrics, "expected_sdf_glyphs"),
            expected_sdf_runs: numberMetric(commandFamilyMetrics, "expected_sdf_runs"),
            expected_camera_bg: numberMetric(commandFamilyMetrics, "expected_camera_bg"),
            image_width: numberMetric(commandFamilyMetrics, "image_width"),
            image_height: numberMetric(commandFamilyMetrics, "image_height"),
         },
      ),
      prefixedBackendCase(
         glyphRunMetrics,
         "web.wasm.webgpu.glyph_run.current",
         "webgpu-glyph-run-current",
         "current",
         {
            expected_glyph_runs: numberMetric(glyphRunMetrics, "expected_glyph_runs"),
            expected_glyphs_per_run: numberMetric(glyphRunMetrics, "expected_glyphs_per_run"),
            expected_glyph_quads: numberMetric(glyphRunMetrics, "expected_glyph_quads"),
            expected_sdf_runs: numberMetric(glyphRunMetrics, "expected_sdf_runs"),
            expected_sdf_glyph_quads: numberMetric(glyphRunMetrics, "expected_sdf_glyph_quads"),
            expected_draw_items: numberMetric(glyphRunMetrics, "expected_draw_items"),
            atlas_width: numberMetric(glyphRunMetrics, "atlas_width"),
            atlas_height: numberMetric(glyphRunMetrics, "atlas_height"),
         },
      ),
      prefixedBackendCase(
         neonMarkerMetrics,
         "web.wasm.webgpu.neon_marker.current",
         "webgpu-neon-marker-current",
         "current",
         {
            expected_markers: numberMetric(neonMarkerMetrics, "expected_markers"),
            expected_draw_items: numberMetric(neonMarkerMetrics, "expected_draw_items"),
         },
      ),
      prefixedBackendCase(
         directSurfaceMetrics,
         "web.wasm.webgpu.direct_surface.current",
         "webgpu-direct-surface-current",
         "current",
         {
            expected_draw_items: numberMetric(directSurfaceMetrics, "expected_draw_items"),
            expected_image_draws: numberMetric(directSurfaceMetrics, "expected_image_draws"),
            columns: numberMetric(directSurfaceMetrics, "columns"),
            image_width: numberMetric(directSurfaceMetrics, "image_width"),
            image_height: numberMetric(directSurfaceMetrics, "image_height"),
         },
      ),
   ];
   let timestampRows = cases.filter(row => row.gpu_timestamp_passes > 0);
   let timestampQuery = stringMetric(timingMetrics, "timestamp_query");
   let timestampCollected = timestampRows.length > 0;
   let timestampPasses = timestampRows.reduce((sum, row) => sum + row.gpu_timestamp_passes, 0);
   let timestampTotalNs = timestampRows.reduce((sum, row) => sum + row.gpu_timestamp_total_ns, 0);
   let warmResourceChurn = warmResourceChurnSummary(cases);
   let gpuTimestampStageBreakdown = gpuTimestampStageBreakdownSummary(cases);
   let wasmAllocationAudit = wasmAllocationSummary(cases);
	   let wasmAllocationInvariance = wasmAllocationInvarianceSummary(wasmAllocationAudit);
	   let backendPathCoverage = backendPathCoverageSummary(cases);
	   let benchmarkMarks = benchmarkMarkSummary(pageReport, traceSummary);
	   let browserStartup = browserStartupSummary(pageReport);

	   return {
	      version: 6,
	      suite: "web-wasm",
      generated_date: args.reportDate,
      browser_target: args.chromeArch
         ? `Chrome ${args.chromeArch} via headless CLI`
         : "Chrome via headless CLI",
      capture_target: args.target,
      browser_environment: pageReport.browser_environment,
      url,
      status: "browser-baseline",
      notes: [
         "BrowserRenderer selected the WebGPU backend through async renderer initialization.",
         "This baseline was collected from a release wasm build served through the static web host.",
         "Production web visual startup is WebGPU-only; unsupported browsers return NOT SUPPORTED instead of drawing through Canvas2D.",
         "The WebGPU ID-mask current row is captured in the default browser report; the upload legacy rows and diagnostic export were retired after same-workload A/B proof.",
         "The WebGPU effect-uniform A/B rows draw the same backdrop scene while comparing one batched dynamic-uniform upload against one queue write per backdrop.",
         "The WebGPU backdrop-batch current row draws separated consecutive backdrops through the shared scene-copy pass after the slower default per-backdrop-copy row was retired.",
         "The WebGPU layer/damage/effects current row draws the nested layer, damage, image, glyph, backdrop, visual-effect, and spinner workload after the slower default legacy rebind/unbatched row was retired.",
         "The WebGPU clean-layer current row draws the retained image/glyph/clip layer through clean cache reuse after the slower default dirty rerender row was retired.",
         "The WebGPU command-family current row draws the generic ImageMesh, NineSlice, and SDF glyph workload after the slower default legacy rebind row was retired, while keeping web CameraBg work unavailable.",
         "The WebGPU glyph-run current row draws the atlas-backed A8 and SDF GlyphRun workload after the slower default legacy rebind row was retired.",
         "The WebGPU direct-surface current row draws the no-effect image workload on the one-pass no-scene-present route after the slower default forced scene-present row was retired.",
         "The standalone draw-item coalescing and draw-state cache diagnostic exports remain non-default diagnostics; the clip-state diagnostic export was retired after repeated startup/package A/B proof.",
         "Pass-family counters provide browser GPU-stage attribution when direct timestamp queries are unavailable.",
         "Warm current-path WebGPU rows are gated against post-warmup resource creation, buffer growth, mesh creation, image-upload temp allocation, and CPU/image scratch growth.",
	         "WASM allocation counters measure Rust allocator activity inside each post-warmup benchmark frame loop and are reported separately from renderer-owned resource churn.",
	         "Chrome startup tracing is captured from a duplicate benchmark-report run so GPU/browser-process activity is tied to the same report workload without perturbing persisted timing rows.",
	         "Browser User Timing marks surround every benchmark family and are persisted to prove the traced report run exercised the expected workload phases.",
	         "Browser startup and static package byte counts are persisted so non-default diagnostic export cleanup can be A/B tested against page-init and artifact-size evidence.",
	      ],
      smoke: {
         platform: pageReport.platform,
         webgpu: pageReport.webgpu,
         webgpu_timing: pageReport.webgpu_timing,
         backend: pageReport.backend,
         render: pageReport.render,
         id_mask_current: pageReport.id_mask_current,
         upload_current: pageReport.upload_current,
         effect_uniform_ab: pageReport.effect_uniform_ab,
         backdrop_batch_current: pageReport.backdrop_batch_current,
         scene3d_ab: pageReport.scene3d_ab,
         mixed_matrix: pageReport.mixed_matrix,
         layer_effects_matrix: pageReport.layer_effects_matrix,
         clean_layer_ab: pageReport.clean_layer_ab,
         command_family_matrix: pageReport.command_family_matrix,
         glyph_run_current: pageReport.glyph_run_current,
         neon_marker_ab: pageReport.neon_marker_ab,
         direct_surface_ab: pageReport.direct_surface_ab,
         capture_target: pageReport.capture_target,
         app_snapshot: pageReport.app_snapshot,
         scene3d_snapshot: pageReport.scene3d_snapshot,
         id_mask_snapshot: pageReport.id_mask_snapshot,
      },
      gpu_stage_attribution: {
         timestamp_query: timestampQuery,
         status: timestampCollected
            ? "timestamp-query-collected"
            : stringMetric(timingMetrics, "gpu_stage_attribution"),
         source: timestampCollected
            ? "adapter.features+renderer.timestamp_writes"
            : stringMetric(timingMetrics, "source"),
         collected_rows: timestampRows.length,
         collected_passes: timestampPasses,
         total_ns: timestampTotalNs,
      },
	      gpu_timestamp_stage_breakdown: gpuTimestampStageBreakdown,
	      browser_startup: browserStartup,
	      browser_trace: traceSummary,
      benchmark_marks: benchmarkMarks,
      warm_resource_churn: warmResourceChurn,
      wasm_allocation_audit: wasmAllocationAudit,
      wasm_allocation_invariance: wasmAllocationInvariance,
      frame_loop_wasm_allocation_stages: frameLoopWasmStageSummary(cases),
      frame_loop_wasm_submit_allocation_stages: frameLoopWasmSubmitStageSummary(cases),
      backend_path_coverage: backendPathCoverage,
      cases,
      id_mask_summary: {
         id: "web.wasm.webgpu.id_mask_compositor.current",
         current_p50_ms: numberMetric(idMaskMetrics, "current_p50_ms"),
         current_render_passes: numberMetric(idMaskMetrics, "current_render_passes"),
         current_buffer_upload_bytes: numberMetric(idMaskMetrics, "current_buffer_upload_bytes"),
         current_uniform_writes: numberMetric(idMaskMetrics, "current_id_mask_uniform_writes"),
         current_uniform_bytes: numberMetric(idMaskMetrics, "current_id_mask_uniform_bytes"),
         current_uniform_slots: numberMetric(idMaskMetrics, "current_id_mask_uniform_slots"),
         vertices: numberMetric(idMaskMetrics, "vertices"),
         vertex_bytes: numberMetric(idMaskMetrics, "vertex_bytes"),
      },
      upload_summary: {
         id: "web.wasm.webgpu.upload.current_dirty",
         glyph_current_p50_ms: numberMetric(uploadMetrics, "glyph_current_p50_ms"),
         image_current_p50_ms: numberMetric(uploadMetrics, "image_current_p50_ms"),
         glyph_current_texture_upload_bytes: numberMetric(uploadMetrics, "glyph_current_texture_upload_bytes"),
         image_current_texture_upload_bytes: numberMetric(uploadMetrics, "image_current_texture_upload_bytes"),
         glyph_current_gpu_timestamp_total_ns: numberMetric(uploadMetrics, "glyph_current_gpu_timestamp_total_ns"),
         image_current_gpu_timestamp_total_ns: numberMetric(uploadMetrics, "image_current_gpu_timestamp_total_ns"),
         atlas_dirty_width: numberMetric(uploadMetrics, "atlas_dirty_width"),
         atlas_dirty_height: numberMetric(uploadMetrics, "atlas_dirty_height"),
         image_dirty_width: numberMetric(uploadMetrics, "image_dirty_width"),
         image_dirty_height: numberMetric(uploadMetrics, "image_dirty_height"),
      },
      effect_uniform_summary: {
         id: "web.wasm.webgpu.effect_uniform.current_batched",
         current_p50_ms: numberMetric(effectUniformMetrics, "current_p50_ms"),
         current_effect_uniform_writes: numberMetric(
            effectUniformMetrics,
            "current_effect_uniform_writes",
         ),
         current_effect_uniform_bytes: numberMetric(
            effectUniformMetrics,
            "current_effect_uniform_bytes",
         ),
         current_effect_uniform_slots: numberMetric(
            effectUniformMetrics,
            "current_effect_uniform_slots",
         ),
         current_backdrop_draws: numberMetric(effectUniformMetrics, "current_backdrop_draws"),
         current_texture_copies: numberMetric(effectUniformMetrics, "current_texture_copies"),
         current_render_passes: numberMetric(effectUniformMetrics, "current_render_passes"),
         current_gpu_timestamp_total_ns: numberMetric(
            effectUniformMetrics,
            "current_gpu_timestamp_total_ns",
         ),
         current_gpu_timestamp_passes: numberMetric(
            effectUniformMetrics,
            "current_gpu_timestamp_passes",
         ),
         expected_backdrops: numberMetric(effectUniformMetrics, "expected_backdrops"),
      },
      backdrop_batch_summary: {
         id: "web.wasm.webgpu.backdrop_batch.current",
         current_p50_ms: numberMetric(backdropBatchMetrics, "current_p50_ms"),
         current_effect_uniform_writes: numberMetric(
            backdropBatchMetrics,
            "current_effect_uniform_writes",
         ),
         current_effect_uniform_slots: numberMetric(
            backdropBatchMetrics,
            "current_effect_uniform_slots",
         ),
         current_backdrop_draws: numberMetric(backdropBatchMetrics, "current_backdrop_draws"),
         current_texture_copies: numberMetric(backdropBatchMetrics, "current_texture_copies"),
         current_render_passes: numberMetric(backdropBatchMetrics, "current_render_passes"),
         current_gpu_timestamp_passes: numberMetric(backdropBatchMetrics, "current_gpu_timestamp_passes"),
         expected_backdrops: numberMetric(backdropBatchMetrics, "expected_backdrops"),
      },
      scene3d_summary: {
         id: "web.wasm.webgpu.scene3d.reused_mesh_vs_recreate_mesh",
         recreate_over_reused: numberMetric(scene3dMetrics, "recreate_over_reused"),
         reused_p50_ms: numberMetric(scene3dMetrics, "reused_p50_ms"),
         recreate_p50_ms: numberMetric(scene3dMetrics, "recreate_p50_ms"),
         reused_mesh3d_creates: numberMetric(scene3dMetrics, "reused_mesh3d_creates"),
         recreate_mesh3d_creates: numberMetric(scene3dMetrics, "recreate_mesh3d_creates"),
         reused_buffer_grows: numberMetric(scene3dMetrics, "reused_buffer_grows"),
         recreate_buffer_grows: numberMetric(scene3dMetrics, "recreate_buffer_grows"),
         reused_cpu_scratch_grows: numberMetric(scene3dMetrics, "reused_cpu_scratch_grows"),
         recreate_cpu_scratch_grows: numberMetric(scene3dMetrics, "recreate_cpu_scratch_grows"),
         reused_cpu_scratch_growth_bytes: numberMetric(scene3dMetrics, "reused_cpu_scratch_growth_bytes"),
         recreate_cpu_scratch_growth_bytes: numberMetric(scene3dMetrics, "recreate_cpu_scratch_growth_bytes"),
         meshes: numberMetric(scene3dMetrics, "meshes"),
         instances: numberMetric(scene3dMetrics, "instances"),
      },
      scene3d_stress_summary: {
         id: "web.wasm.webgpu.scene3d.stress_reused_mesh_vs_stress_recreate_mesh",
         recreate_over_reused: numberMetric(scene3dMetrics, "stress_recreate_over_reused"),
         reused_p50_ms: numberMetric(scene3dMetrics, "stress_reused_p50_ms"),
         recreate_p50_ms: numberMetric(scene3dMetrics, "stress_recreate_p50_ms"),
         reused_mesh3d_creates: numberMetric(scene3dMetrics, "stress_reused_mesh3d_creates"),
         recreate_mesh3d_creates: numberMetric(scene3dMetrics, "stress_recreate_mesh3d_creates"),
         reused_buffer_grows: numberMetric(scene3dMetrics, "stress_reused_buffer_grows"),
         recreate_buffer_grows: numberMetric(scene3dMetrics, "stress_recreate_buffer_grows"),
         reused_cpu_scratch_grows: numberMetric(scene3dMetrics, "stress_reused_cpu_scratch_grows"),
         recreate_cpu_scratch_grows: numberMetric(scene3dMetrics, "stress_recreate_cpu_scratch_grows"),
         reused_cpu_scratch_growth_bytes: numberMetric(scene3dMetrics, "stress_reused_cpu_scratch_growth_bytes"),
         recreate_cpu_scratch_growth_bytes: numberMetric(scene3dMetrics, "stress_recreate_cpu_scratch_growth_bytes"),
         meshes: numberMetric(scene3dMetrics, "stress_meshes"),
         instances: numberMetric(scene3dMetrics, "stress_instances"),
      },
      mixed_summary: {
         id: "web.wasm.webgpu.mixed_text_image_effects.current",
         current_p50_ms: numberMetric(mixedMetrics, "current_p50_ms"),
         current_draw_items: numberMetric(mixedMetrics, "current_draw_items"),
         current_draw_pipeline_binds: numberMetric(mixedMetrics, "current_draw_pipeline_binds"),
         current_draw_bind_group_binds: numberMetric(mixedMetrics, "current_draw_bind_group_binds"),
         current_draw_scissor_sets: numberMetric(mixedMetrics, "current_draw_scissor_sets"),
         current_effect_uniform_writes: numberMetric(mixedMetrics, "current_effect_uniform_writes"),
         current_texture_copies: numberMetric(mixedMetrics, "current_texture_copies"),
         current_render_passes: numberMetric(mixedMetrics, "current_render_passes"),
         current_glyph_quads: numberMetric(mixedMetrics, "current_glyph_quads"),
         current_image_draws: numberMetric(mixedMetrics, "current_image_draws"),
         image_tiles: numberMetric(mixedMetrics, "image_tiles"),
         current_backdrop_draws: numberMetric(mixedMetrics, "current_backdrop_draws"),
         current_visual_effect_draws: numberMetric(mixedMetrics, "current_visual_effect_draws"),
         current_layer_draws: numberMetric(mixedMetrics, "current_layer_draws"),
         current_damage_rects: numberMetric(mixedMetrics, "current_damage_rects"),
      },
      layer_effects_summary: {
         id: "web.wasm.webgpu.layer_damage_effects.current",
         current_p50_ms: numberMetric(layerEffectsMetrics, "current_p50_ms"),
         current_draw_items: numberMetric(layerEffectsMetrics, "current_draw_items"),
         current_draw_pipeline_binds: numberMetric(layerEffectsMetrics, "current_draw_pipeline_binds"),
         current_draw_bind_group_binds: numberMetric(layerEffectsMetrics, "current_draw_bind_group_binds"),
         current_draw_scissor_sets: numberMetric(layerEffectsMetrics, "current_draw_scissor_sets"),
         current_effect_uniform_writes: numberMetric(layerEffectsMetrics, "current_effect_uniform_writes"),
         current_texture_copies: numberMetric(layerEffectsMetrics, "current_texture_copies"),
         current_render_passes: numberMetric(layerEffectsMetrics, "current_render_passes"),
         current_glyph_quads: numberMetric(layerEffectsMetrics, "current_glyph_quads"),
         current_image_draws: numberMetric(layerEffectsMetrics, "current_image_draws"),
         image_tiles: numberMetric(layerEffectsMetrics, "image_tiles"),
         current_backdrop_draws: numberMetric(layerEffectsMetrics, "current_backdrop_draws"),
         current_visual_effect_draws: numberMetric(layerEffectsMetrics, "current_visual_effect_draws"),
         current_spinner_draws: numberMetric(layerEffectsMetrics, "current_spinner_draws"),
         current_layer_draws: numberMetric(layerEffectsMetrics, "current_layer_draws"),
         current_damage_rects: numberMetric(layerEffectsMetrics, "current_damage_rects"),
         expected_layers: numberMetric(layerEffectsMetrics, "expected_layers"),
         expected_damage_rects: numberMetric(layerEffectsMetrics, "expected_damage_rects"),
         expected_backdrops: numberMetric(layerEffectsMetrics, "expected_backdrops"),
      },
      clean_layer_summary: {
         id: "web.wasm.webgpu.clean_layer.clean_reuse",
         clean_p50_ms: numberMetric(cleanLayerMetrics, "clean_p50_ms"),
         clean_draw_items: numberMetric(cleanLayerMetrics, "clean_draw_items"),
         clean_draw_pipeline_binds: numberMetric(cleanLayerMetrics, "clean_draw_pipeline_binds"),
         clean_draw_bind_group_binds: numberMetric(cleanLayerMetrics, "clean_draw_bind_group_binds"),
         clean_draw_scissor_sets: numberMetric(cleanLayerMetrics, "clean_draw_scissor_sets"),
         clean_layer_cache_hits: numberMetric(cleanLayerMetrics, "clean_layer_cache_hits"),
         clean_layer_cache_misses: numberMetric(cleanLayerMetrics, "clean_layer_cache_misses"),
         clean_layer_cache_skipped_draws: numberMetric(cleanLayerMetrics, "clean_layer_cache_skipped_draws"),
         clean_layer_passes: numberMetric(cleanLayerMetrics, "clean_layer_passes"),
         clean_render_passes: numberMetric(cleanLayerMetrics, "clean_render_passes"),
         clean_gpu_timestamp_total_ns: numberMetric(cleanLayerMetrics, "clean_gpu_timestamp_total_ns"),
         glyphs: numberMetric(cleanLayerMetrics, "glyphs"),
         image_tiles: numberMetric(cleanLayerMetrics, "image_tiles"),
         expected_layers: numberMetric(cleanLayerMetrics, "expected_layers"),
         expected_clean_hits: numberMetric(cleanLayerMetrics, "expected_clean_hits"),
      },
      command_family_summary: {
         id: "web.wasm.webgpu.command_family_matrix.current",
         current_p50_ms: numberMetric(commandFamilyMetrics, "current_p50_ms"),
         current_draw_items: numberMetric(commandFamilyMetrics, "current_draw_items"),
         current_draw_pipeline_binds: numberMetric(commandFamilyMetrics, "current_draw_pipeline_binds"),
         current_draw_bind_group_binds: numberMetric(commandFamilyMetrics, "current_draw_bind_group_binds"),
         current_draw_scissor_sets: numberMetric(commandFamilyMetrics, "current_draw_scissor_sets"),
         current_image_mesh_draws: numberMetric(commandFamilyMetrics, "current_image_mesh_draws"),
         current_nine_slice_draws: numberMetric(commandFamilyMetrics, "current_nine_slice_draws"),
         current_sdf_glyph_quads: numberMetric(commandFamilyMetrics, "current_sdf_glyph_quads"),
         current_camera_bg_draws: numberMetric(commandFamilyMetrics, "current_camera_bg_draws"),
         expected_image_meshes: numberMetric(commandFamilyMetrics, "expected_image_meshes"),
         expected_nine_slices: numberMetric(commandFamilyMetrics, "expected_nine_slices"),
         expected_sdf_glyphs: numberMetric(commandFamilyMetrics, "expected_sdf_glyphs"),
         expected_sdf_runs: numberMetric(commandFamilyMetrics, "expected_sdf_runs"),
         expected_camera_bg: numberMetric(commandFamilyMetrics, "expected_camera_bg"),
      },
      glyph_run_summary: {
         id: "web.wasm.webgpu.glyph_run.current",
         current_p50_ms: numberMetric(glyphRunMetrics, "current_p50_ms"),
         current_draw_items: numberMetric(glyphRunMetrics, "current_draw_items"),
         current_glyph_quads: numberMetric(glyphRunMetrics, "current_glyph_quads"),
         current_sdf_glyph_quads: numberMetric(glyphRunMetrics, "current_sdf_glyph_quads"),
         current_render_passes: numberMetric(glyphRunMetrics, "current_render_passes"),
         current_draw_passes: numberMetric(glyphRunMetrics, "current_draw_passes"),
         current_draw_pipeline_binds: numberMetric(glyphRunMetrics, "current_draw_pipeline_binds"),
         current_draw_bind_group_binds: numberMetric(glyphRunMetrics, "current_draw_bind_group_binds"),
         current_draw_scissor_sets: numberMetric(glyphRunMetrics, "current_draw_scissor_sets"),
         expected_glyph_runs: numberMetric(glyphRunMetrics, "expected_glyph_runs"),
         expected_glyphs_per_run: numberMetric(glyphRunMetrics, "expected_glyphs_per_run"),
         expected_glyph_quads: numberMetric(glyphRunMetrics, "expected_glyph_quads"),
         expected_sdf_runs: numberMetric(glyphRunMetrics, "expected_sdf_runs"),
         expected_sdf_glyph_quads: numberMetric(glyphRunMetrics, "expected_sdf_glyph_quads"),
         expected_draw_items: numberMetric(glyphRunMetrics, "expected_draw_items"),
      },
      neon_marker_summary: {
         id: "web.wasm.webgpu.neon_marker.current",
         current_p50_ms: numberMetric(neonMarkerMetrics, "current_p50_ms"),
         current_draw_items: numberMetric(neonMarkerMetrics, "current_draw_items"),
         current_solid_tris: numberMetric(neonMarkerMetrics, "current_solid_tris"),
         current_draw_pipeline_binds: numberMetric(neonMarkerMetrics, "current_draw_pipeline_binds"),
         current_draw_bind_group_binds: numberMetric(
            neonMarkerMetrics,
            "current_draw_bind_group_binds",
         ),
         current_draw_scissor_sets: numberMetric(neonMarkerMetrics, "current_draw_scissor_sets"),
         expected_markers: numberMetric(neonMarkerMetrics, "expected_markers"),
         expected_draw_items: numberMetric(neonMarkerMetrics, "expected_draw_items"),
      },
      direct_surface_summary: {
         id: "web.wasm.webgpu.direct_surface.current",
         current_p50_ms: numberMetric(directSurfaceMetrics, "current_p50_ms"),
         current_draw_items: numberMetric(directSurfaceMetrics, "current_draw_items"),
         current_image_draws: numberMetric(directSurfaceMetrics, "current_image_draws"),
         current_render_passes: numberMetric(directSurfaceMetrics, "current_render_passes"),
         current_draw_passes: numberMetric(directSurfaceMetrics, "current_draw_passes"),
         current_clear_passes: numberMetric(directSurfaceMetrics, "current_clear_passes"),
         current_present_passes: numberMetric(directSurfaceMetrics, "current_present_passes"),
         current_texture_copies: numberMetric(directSurfaceMetrics, "current_texture_copies"),
         current_gpu_timestamp_total_ns: numberMetric(directSurfaceMetrics, "current_gpu_timestamp_total_ns"),
         current_gpu_timestamp_passes: numberMetric(directSurfaceMetrics, "current_gpu_timestamp_passes"),
         expected_draw_items: numberMetric(directSurfaceMetrics, "expected_draw_items"),
         expected_image_draws: numberMetric(directSurfaceMetrics, "expected_image_draws"),
      },
      pixel_check: pixelReport,
   };
}

function renderMarkdown(report)
{
   let lines = [];
   lines.push("# Oxide WebAssembly Browser Baseline");
   lines.push("");
   lines.push(`Date: ${report.generated_date}`);
   lines.push("");
   lines.push(`Target: ${report.browser_target}`);
   lines.push("");
   lines.push(`Capture target: \`${report.capture_target}\``);
   lines.push("");
   lines.push(`URL: \`${report.url}\``);
   lines.push("");
   lines.push("Status: browser-baseline. This is the browser-specific WebGPU/WebAssembly baseline for the current web backend. It is not an official device parity report.");
   lines.push("");
   lines.push("## Smoke");
   lines.push("");
   lines.push("| Check | Result |");
   lines.push("| --- | --- |");
   lines.push(`| Platform | \`${report.smoke.platform}\` |`);
   lines.push(`| WebGPU probe | \`${report.smoke.webgpu}\` |`);
   lines.push(`| WebGPU timing | \`${report.smoke.webgpu_timing}\` |`);
   lines.push(`| Renderer backend | \`${report.smoke.backend}\` |`);
   lines.push(`| Renderer | \`${report.smoke.render}\` |`);
   lines.push(`| Capture target | \`${report.smoke.capture_target}\` |`);
   if (report.smoke.app_snapshot) {
      lines.push(`| App snapshot | \`${report.smoke.app_snapshot}\` |`);
   }
	   if (report.smoke.id_mask_snapshot) {
	      lines.push(`| ID-mask snapshot | \`${report.smoke.id_mask_snapshot}\` |`);
	   }
	   lines.push("");
	   lines.push("## Browser Startup");
	   lines.push("");
	   lines.push("| Field | Value |");
	   lines.push("| --- | ---: |");
	   lines.push(`| Page start ms | ${report.browser_startup.page_start_ms.toFixed(3)} |`);
	   lines.push(`| WASM init start ms | ${report.browser_startup.wasm_init_start_ms.toFixed(3)} |`);
	   lines.push(`| WASM init ms | ${report.browser_startup.wasm_init_ms.toFixed(3)} |`);
	   lines.push(`| App init start ms | ${report.browser_startup.app_init_start_ms.toFixed(3)} |`);
	   lines.push(`| App init ms | ${report.browser_startup.app_init_ms.toFixed(3)} |`);
	   lines.push(`| First frame start ms | ${report.browser_startup.first_frame_start_ms.toFixed(3)} |`);
	   lines.push(`| First frame ms | ${report.browser_startup.first_frame_ms.toFixed(3)} |`);
	   lines.push(`| Report ready ms | ${report.browser_startup.report_ready_ms.toFixed(3)} |`);
	   lines.push(`| WASM memory bytes | ${report.browser_startup.wasm_memory_bytes} |`);
	   lines.push(`| Package bytes | ${report.browser_startup.package_bytes} |`);
	   lines.push("");
	   lines.push("### Browser Package Files");
	   lines.push("");
	   lines.push("| File | Kind | Bytes |");
	   lines.push("| --- | --- | ---: |");
	   for (let file of report.browser_startup.files) {
	      lines.push(`| \`${file.path}\` | \`${file.kind}\` | ${file.bytes} |`);
	   }
	   lines.push("");
	   lines.push("## Cases");
   lines.push("");
   lines.push("| Case | Variant | Samples | Frames/Sample | Frames | p50 ms | p95 ms | p99 ms | Peak ms | Avg ms | Unit | Notes |");
   lines.push("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |");
   for (let row of report.cases) {
      let notes = [];
      if (row.draws !== undefined) {
         notes.push(`draws=${row.draws}`);
      }
      if (row.draw_items !== undefined) {
         notes.push(`draw_items=${row.draw_items}`);
      }
      if (row.draw_pipeline_binds !== undefined) {
         notes.push(`draw_pipeline_binds=${row.draw_pipeline_binds}`);
      }
      if (row.draw_bind_group_binds !== undefined) {
         notes.push(`draw_bind_group_binds=${row.draw_bind_group_binds}`);
      }
      if (row.draw_scissor_sets !== undefined) {
         notes.push(`draw_scissor_sets=${row.draw_scissor_sets}`);
      }
      if (row.solid_tris !== undefined) {
         notes.push(`solid_tris=${row.solid_tris}`);
      }
      if (row.image_draws !== undefined) {
         notes.push(`image_draws=${row.image_draws}`);
      }
      if (row.image_mesh_draws !== undefined) {
         notes.push(`image_mesh_draws=${row.image_mesh_draws}`);
      }
      if (row.nine_slice_draws !== undefined) {
         notes.push(`nine_slice_draws=${row.nine_slice_draws}`);
      }
      if (row.glyph_quads !== undefined) {
         notes.push(`glyph_quads=${row.glyph_quads}`);
      }
      if (row.sdf_glyph_quads !== undefined) {
         notes.push(`sdf_glyph_quads=${row.sdf_glyph_quads}`);
      }
      if (row.clip_depth_peak !== undefined) {
         notes.push(`clip_depth_peak=${row.clip_depth_peak}`);
      }
      if (row.damage_rects !== undefined) {
         notes.push(`damage_rects=${row.damage_rects}`);
      }
      if (row.layer_draws !== undefined) {
         notes.push(`layer_draws=${row.layer_draws}`);
      }
      if (row.scene3d_draws !== undefined) {
         notes.push(`scene3d_draws=${row.scene3d_draws}`);
      }
      if (row.id_mask_draws !== undefined) {
         notes.push(`id_mask_draws=${row.id_mask_draws}`);
      }
      if (row.backdrop_draws !== undefined) {
         notes.push(`backdrop_draws=${row.backdrop_draws}`);
      }
      if (row.visual_effect_draws !== undefined) {
         notes.push(`visual_effect_draws=${row.visual_effect_draws}`);
      }
      if (row.effect_uniform_writes !== undefined) {
         notes.push(`effect_uniform_writes=${row.effect_uniform_writes}`);
      }
      if (row.effect_uniform_bytes !== undefined) {
         notes.push(`effect_uniform_bytes=${row.effect_uniform_bytes}`);
      }
      if (row.effect_uniform_slots !== undefined) {
         notes.push(`effect_uniform_slots=${row.effect_uniform_slots}`);
      }
      if (row.spinner_draws !== undefined) {
         notes.push(`spinner_draws=${row.spinner_draws}`);
      }
      if (row.camera_bg_draws !== undefined) {
         notes.push(`camera_bg_draws=${row.camera_bg_draws}`);
      }
      if (row.render_passes !== undefined) {
         notes.push(`render_passes=${row.render_passes}`);
      }
      if (row.clear_passes !== undefined || row.draw_passes !== undefined || row.present_passes !== undefined) {
         notes.push(`passes=clear:${row.clear_passes}/draw:${row.draw_passes}/scene3d:${row.scene3d_passes}/scene3d_overlay:${row.scene3d_overlay_passes}/idmask:${row.id_mask_raster_passes}+${row.id_mask_field_seed_passes}+${row.id_mask_field_jump_passes}+${row.id_mask_compositor_passes}/present:${row.present_passes}`);
      }
      if (row.texture_copies !== undefined) {
         notes.push(`texture_copies=${row.texture_copies}`);
      }
      if (row.command_buffers !== undefined) {
         notes.push(`command_buffers=${row.command_buffers}`);
      }
      if (row.gpu_timestamp_passes !== undefined) {
         notes.push(`gpu_ts_passes=${row.gpu_timestamp_passes}`);
      }
      if (row.gpu_timestamp_total_ns !== undefined) {
         notes.push(`gpu_ts_total_ns=${row.gpu_timestamp_total_ns}`);
      }
      if (row.gpu_timestamp_max_pass_ns !== undefined) {
         notes.push(`gpu_ts_max_ns=${row.gpu_timestamp_max_pass_ns}`);
      }
      if (row.buffer_upload_bytes !== undefined) {
         notes.push(`buffer_upload_bytes=${row.buffer_upload_bytes}`);
      }
      if (row.texture_upload_bytes !== undefined) {
         notes.push(`texture_upload_bytes=${row.texture_upload_bytes}`);
      }
      if (row.buffer_grows !== undefined) {
         notes.push(`buffer_grows=${row.buffer_grows}`);
      }
      if (row.texture_creates !== undefined) {
         notes.push(`texture_creates=${row.texture_creates}`);
      }
      if (row.bind_group_creates !== undefined) {
         notes.push(`bind_group_creates=${row.bind_group_creates}`);
      }
      if (row.pipeline_creates !== undefined) {
         notes.push(`pipeline_creates=${row.pipeline_creates}`);
      }
      if (row.sampler_creates !== undefined) {
         notes.push(`sampler_creates=${row.sampler_creates}`);
      }
      if (row.mesh3d_creates !== undefined) {
         notes.push(`mesh3d_creates=${row.mesh3d_creates}`);
      }
      if (row.wasm_alloc_count !== undefined) {
         notes.push(`wasm_alloc_count=${row.wasm_alloc_count}`);
      }
      if (row.wasm_alloc_bytes !== undefined) {
         notes.push(`wasm_alloc_bytes=${row.wasm_alloc_bytes}`);
      }
      if (row.wasm_realloc_count !== undefined) {
         notes.push(`wasm_realloc_count=${row.wasm_realloc_count}`);
      }
      if (row.wasm_realloc_grow_bytes !== undefined) {
         notes.push(`wasm_realloc_grow_bytes=${row.wasm_realloc_grow_bytes}`);
      }
      if (row.wasm_allocating_frames !== undefined) {
         notes.push(`wasm_allocating_frames=${row.wasm_allocating_frames}`);
      }
      if (row.image_upload_temp_allocs !== undefined) {
         notes.push(`image_upload_temp_allocs=${row.image_upload_temp_allocs}`);
      }
      if (row.image_upload_temp_bytes !== undefined) {
         notes.push(`image_upload_temp_bytes=${row.image_upload_temp_bytes}`);
      }
      if (row.image_upload_scratch_bytes !== undefined) {
         notes.push(`image_upload_scratch_bytes=${row.image_upload_scratch_bytes}`);
      }
      if (row.image_upload_scratch_grows !== undefined) {
         notes.push(`image_upload_scratch_grows=${row.image_upload_scratch_grows}`);
      }
      if (row.cpu_scratch_bytes !== undefined) {
         notes.push(`cpu_scratch_bytes=${row.cpu_scratch_bytes}`);
      }
      if (row.cpu_scratch_grows !== undefined) {
         notes.push(`cpu_scratch_grows=${row.cpu_scratch_grows}`);
      }
      if (row.cpu_scratch_growth_bytes !== undefined) {
         notes.push(`cpu_scratch_growth_bytes=${row.cpu_scratch_growth_bytes}`);
      }
      if (row.atlas_width !== undefined) {
         notes.push(`atlas=${row.atlas_width}x${row.atlas_height}`);
      }
      if (row.image_width !== undefined) {
         notes.push(`image=${row.image_width}x${row.image_height}`);
      }
      if (row.dirty_width !== undefined) {
         notes.push(`dirty=${row.dirty_width}x${row.dirty_height}`);
      }
      if (row.glyphs !== undefined) {
         notes.push(`glyphs=${row.glyphs}`);
      }
      if (row.image_tiles !== undefined) {
         notes.push(`image_tiles=${row.image_tiles}`);
      }
      if (row.expected_layers !== undefined) {
         notes.push(`expected_layers=${row.expected_layers}`);
      }
      if (row.expected_damage_rects !== undefined) {
         notes.push(`expected_damage_rects=${row.expected_damage_rects}`);
      }
      if (row.expected_image_meshes !== undefined) {
         notes.push(`expected_image_meshes=${row.expected_image_meshes}`);
      }
      if (row.expected_nine_slices !== undefined) {
         notes.push(`expected_nine_slices=${row.expected_nine_slices}`);
      }
      if (row.expected_sdf_glyphs !== undefined) {
         notes.push(`expected_sdf_glyphs=${row.expected_sdf_glyphs}`);
      }
      if (row.expected_sdf_runs !== undefined) {
         notes.push(`expected_sdf_runs=${row.expected_sdf_runs}`);
      }
      if (row.expected_camera_bg !== undefined) {
         notes.push(`expected_camera_bg=${row.expected_camera_bg}`);
      }
      if (row.expected_draw_items !== undefined) {
         notes.push(`expected_draw_items=${row.expected_draw_items}`);
      }
      if (row.expected_clip_runs !== undefined) {
         notes.push(`expected_clip_runs=${row.expected_clip_runs}`);
      }
      if (row.expected_clip_depth !== undefined) {
         notes.push(`expected_clip_depth=${row.expected_clip_depth}`);
      }
      if (row.expected_backdrops !== undefined) {
         notes.push(`expected_backdrops=${row.expected_backdrops}`);
      }
      if (row.updates !== undefined) {
         notes.push(`updates=${row.updates}`);
      }
      if (row.vertices !== undefined) {
         notes.push(`vertices=${row.vertices}`);
      }
      if (row.vertex_bytes !== undefined) {
         notes.push(`vertex_bytes=${row.vertex_bytes}`);
      }
      if (row.meshes !== undefined) {
         notes.push(`meshes=${row.meshes}`);
      }
      if (row.instances !== undefined) {
         notes.push(`instances=${row.instances}`);
      }
      if (row.missed_frame_ratio_120hz !== undefined) {
         notes.push(`missed120=${row.missed_frame_ratio_120hz.toFixed(3)}`);
      }
      if (row.hitch_ratio_120hz !== undefined) {
         notes.push(`hitch120=${row.hitch_ratio_120hz.toFixed(3)}`);
      }
      if (row.missed_frame_ratio_60hz !== undefined) {
         notes.push(`missed60=${row.missed_frame_ratio_60hz.toFixed(3)}`);
      }
      if (row.hitch_ratio_60hz !== undefined) {
         notes.push(`hitch60=${row.hitch_ratio_60hz.toFixed(3)}`);
      }
      let p50Ms = row.p50_ms ?? row.cpu_submit_p50_ms;
      let p95Ms = row.p95_ms ?? row.cpu_submit_p95_ms;
      let p99Ms = row.p99_ms ?? row.cpu_submit_p99_ms;
      let peakMs = row.peak_ms ?? row.cpu_submit_peak_ms;
      let avgMs = row.avg_ms ?? row.cpu_submit_avg_ms;
      lines.push(`| \`${row.id}\` | \`${row.variant}\` | ${row.samples} | ${row.frames_per_sample ?? 1} | ${row.frames} | ${p50Ms.toFixed(3)} | ${p95Ms.toFixed(3)} | ${p99Ms.toFixed(3)} | ${peakMs.toFixed(3)} | ${avgMs.toFixed(3)} | ${row.unit} | \`${notes.join(";") || "-"}\` |`);
   }
   lines.push("");
   lines.push("## GPU Stage Attribution");
   lines.push("");
   lines.push("| Field | Value |");
   lines.push("| --- | --- |");
   lines.push(`| Timestamp query | \`${report.gpu_stage_attribution.timestamp_query}\` |`);
   lines.push(`| Status | \`${report.gpu_stage_attribution.status}\` |`);
   lines.push(`| Source | \`${report.gpu_stage_attribution.source}\` |`);
   lines.push(`| Collected rows | \`${report.gpu_stage_attribution.collected_rows}\` |`);
   lines.push(`| Collected passes | \`${report.gpu_stage_attribution.collected_passes}\` |`);
   lines.push(`| Total ns | \`${report.gpu_stage_attribution.total_ns}\` |`);
   lines.push("");
   lines.push("### GPU Timestamp Stage Breakdown");
   lines.push("");
   lines.push("| Stage | Passes | Timestamp ns |");
   lines.push("| --- | ---: | ---: |");
   for (let stage of report.gpu_timestamp_stage_breakdown.stages) {
      lines.push(`| \`${stage.stage}\` | ${stage.pass_count} | ${stage.timestamp_ns} |`);
   }
   lines.push("");
   lines.push("### GPU Timestamp Row Reconciliation");
   lines.push("");
   lines.push("| Row | Render Passes | Timestamp Passes | Timestamp ns | Family Passes | Family Timestamp ns |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: |");
   for (let row of report.gpu_timestamp_stage_breakdown.row_details) {
      lines.push(`| \`${row.id}\` | ${row.render_passes} | ${row.gpu_timestamp_passes} | ${row.gpu_timestamp_total_ns} | ${row.family_passes} | ${row.family_timestamp_ns} |`);
   }
   lines.push("");
   lines.push("## Browser Trace");
   lines.push("");
   lines.push("| Field | Value |");
   lines.push("| --- | --- |");
   lines.push(`| Status | \`${report.browser_trace.status}\` |`);
   lines.push(`| Artifact | \`${report.browser_trace.artifact}\` |`);
   lines.push(`| Capture phase | \`${report.browser_trace.capture_phase}\` |`);
   lines.push(`| Timing source | \`${report.browser_trace.timing_source}\` |`);
   lines.push(`| Events | \`${report.browser_trace.events}\` |`);
   lines.push(`| GPU-related events | \`${report.browser_trace.gpu_related_events}\` |`);
   lines.push(`| WebGPU/Dawn events | \`${report.browser_trace.webgpu_related_events}\` |`);
   lines.push(`| ANGLE events | \`${report.browser_trace.angle_related_events}\` |`);
   lines.push(`| Renderer events | \`${report.browser_trace.renderer_related_events}\` |`);
   lines.push(`| Duration us | \`${report.browser_trace.duration_us}\` |`);
   lines.push(`| Category count | \`${report.browser_trace.category_count}\` |`);
   lines.push(`| Sample categories | \`${report.browser_trace.sample_categories.join(",")}\` |`);
   lines.push(`| Benchmark trace mark status | \`${report.browser_trace.benchmark_trace_mark_status}\` |`);
   lines.push(`| Benchmark trace mark events | \`${report.browser_trace.benchmark_trace_mark_count}\` |`);
   lines.push(`| Benchmark trace mark labels | \`${report.browser_trace.benchmark_trace_mark_labels.join(",")}\` |`);
   lines.push("");
   lines.push("### Browser Trace Benchmark Intervals");
   lines.push("");
   lines.push("| Mark | Duration us | Events | GPU events | WebGPU/Dawn events | Renderer events | Event duration us |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: |");
   for (let interval of report.browser_trace.benchmark_trace_intervals) {
      lines.push(`| \`${interval.id}\` | ${interval.duration_us} | ${interval.event_count} | ${interval.gpu_related_events} | ${interval.webgpu_related_events} | ${interval.renderer_related_events} | ${interval.event_duration_us} |`);
   }
   lines.push("");
   lines.push("## Benchmark Marks");
   lines.push("");
   lines.push("| Mark | Duration ms | Trace label | WASM before bytes | WASM after bytes | WASM growth bytes | JS heap before bytes | JS heap after bytes | JS heap growth bytes | JS heap sampled | GC exposed |");
   lines.push("| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   for (let mark of report.benchmark_marks.marks) {
      let traced = report.benchmark_marks.traced_labels.includes(mark.id) ? "yes" : "no";
      lines.push(`| \`${mark.id}\` | ${mark.duration_ms.toFixed(3)} | \`${traced}\` | ${mark.wasm_memory_before_bytes} | ${mark.wasm_memory_after_bytes} | ${mark.wasm_memory_growth_bytes} | ${mark.js_heap_before_bytes} | ${mark.js_heap_after_bytes} | ${mark.js_heap_growth_bytes} | ${mark.js_heap_sample_supported} | ${mark.js_heap_gc_available} |`);
   }
   lines.push("");
   lines.push("## Warm Resource Churn");
   lines.push("");
   lines.push("| Check | Rows | Buffer Grows | Texture Creates | Bind Groups | Pipelines | Samplers | Meshes | Temp Allocs | Temp Bytes | Image Scratch Grows | CPU Scratch Grows | CPU Scratch Growth Bytes |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.warm_resource_churn.id}\` | ${report.warm_resource_churn.checked_rows} checked / ${report.warm_resource_churn.excluded_rows} excluded | ${report.warm_resource_churn.total_buffer_grows} | ${report.warm_resource_churn.total_texture_creates} | ${report.warm_resource_churn.total_bind_group_creates} | ${report.warm_resource_churn.total_pipeline_creates} | ${report.warm_resource_churn.total_sampler_creates} | ${report.warm_resource_churn.total_mesh3d_creates} | ${report.warm_resource_churn.total_image_upload_temp_allocs} | ${report.warm_resource_churn.total_image_upload_temp_bytes} | ${report.warm_resource_churn.total_image_upload_scratch_grows} | ${report.warm_resource_churn.total_cpu_scratch_grows} | ${report.warm_resource_churn.total_cpu_scratch_growth_bytes} |`);
   lines.push("");
   lines.push("### Warm Resource Churn Rows");
   lines.push("");
   lines.push("| Row | Buffer Grows | Texture Creates | Bind Groups | Pipelines | Samplers | Meshes | Temp Allocs | Temp Bytes | Image Scratch Grows | CPU Scratch Grows | CPU Scratch Growth Bytes |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   for (let row of report.warm_resource_churn.row_details) {
      lines.push(`| \`${row.id}\` | ${row.buffer_grows} | ${row.texture_creates} | ${row.bind_group_creates} | ${row.pipeline_creates} | ${row.sampler_creates} | ${row.mesh3d_creates} | ${row.image_upload_temp_allocs} | ${row.image_upload_temp_bytes} | ${row.image_upload_scratch_grows} | ${row.cpu_scratch_grows} | ${row.cpu_scratch_growth_bytes} |`);
   }
   lines.push("");
   lines.push("### Warm GPU Resource Family Churn");
   lines.push("");
   lines.push("| Row | Draw Buffers | Image Textures | Image Bind Groups | Target Textures | Target Bind Groups | Scene3D Buffers | Scene3D Bind Groups | Effect Buffers | Effect Bind Groups | ID Mask Textures | ID Mask Buffers | ID Mask Bind Groups |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.warm_resource_churn.id}\` | ${report.warm_resource_churn.total_draw_buffer_grows} | ${report.warm_resource_churn.total_image_texture_creates} | ${report.warm_resource_churn.total_image_bind_group_creates} | ${report.warm_resource_churn.total_target_texture_creates} | ${report.warm_resource_churn.total_target_bind_group_creates} | ${report.warm_resource_churn.total_scene3d_buffer_grows} | ${report.warm_resource_churn.total_scene3d_bind_group_creates} | ${report.warm_resource_churn.total_effect_buffer_grows} | ${report.warm_resource_churn.total_effect_bind_group_creates} | ${report.warm_resource_churn.total_id_mask_texture_creates} | ${report.warm_resource_churn.total_id_mask_buffer_grows} | ${report.warm_resource_churn.total_id_mask_bind_group_creates} |`);
   for (let row of report.warm_resource_churn.row_details) {
      lines.push(`| \`${row.id}\` | ${row.draw_buffer_grows} | ${row.image_texture_creates} | ${row.image_bind_group_creates} | ${row.target_texture_creates} | ${row.target_bind_group_creates} | ${row.scene3d_buffer_grows} | ${row.scene3d_bind_group_creates} | ${row.effect_buffer_grows} | ${row.effect_bind_group_creates} | ${row.id_mask_texture_creates} | ${row.id_mask_buffer_grows} | ${row.id_mask_bind_group_creates} |`);
   }
   lines.push("");
   lines.push("### Warm Scratch Family Churn");
   lines.push("");
   lines.push("| Row | Draw Grows | Draw Bytes | Scene3D Grows | Scene3D Bytes | Effect Grows | Effect Bytes | ID Mask Grows | ID Mask Bytes | Image Upload Grows | Image Upload Bytes | Resource Table Grows | Resource Table Bytes |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.warm_resource_churn.id}\` | ${report.warm_resource_churn.total_cpu_draw_scratch_grows} | ${report.warm_resource_churn.total_cpu_draw_scratch_growth_bytes} | ${report.warm_resource_churn.total_cpu_scene3d_scratch_grows} | ${report.warm_resource_churn.total_cpu_scene3d_scratch_growth_bytes} | ${report.warm_resource_churn.total_cpu_effect_scratch_grows} | ${report.warm_resource_churn.total_cpu_effect_scratch_growth_bytes} | ${report.warm_resource_churn.total_cpu_id_mask_scratch_grows} | ${report.warm_resource_churn.total_cpu_id_mask_scratch_growth_bytes} | ${report.warm_resource_churn.total_cpu_image_upload_scratch_grows} | ${report.warm_resource_churn.total_cpu_image_upload_scratch_growth_bytes} | ${report.warm_resource_churn.total_cpu_resource_table_scratch_grows} | ${report.warm_resource_churn.total_cpu_resource_table_scratch_growth_bytes} |`);
   for (let row of report.warm_resource_churn.row_details) {
      lines.push(`| \`${row.id}\` | ${row.cpu_draw_scratch_grows} | ${row.cpu_draw_scratch_growth_bytes} | ${row.cpu_scene3d_scratch_grows} | ${row.cpu_scene3d_scratch_growth_bytes} | ${row.cpu_effect_scratch_grows} | ${row.cpu_effect_scratch_growth_bytes} | ${row.cpu_id_mask_scratch_grows} | ${row.cpu_id_mask_scratch_growth_bytes} | ${row.cpu_image_upload_scratch_grows} | ${row.cpu_image_upload_scratch_growth_bytes} | ${row.cpu_resource_table_scratch_grows} | ${row.cpu_resource_table_scratch_growth_bytes} |`);
   }
   lines.push("");
   lines.push("## WASM Allocation Audit");
   lines.push("");
   lines.push("| Check | Rows | Total Allocs | Total Bytes | Reallocs | Realloc Grow Bytes | Max Allocs/Frame | Max Bytes/Frame | Max Peak Frame Bytes | Budget Allocs/Frame | Budget Bytes/Frame |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.wasm_allocation_audit.id}\` | ${report.wasm_allocation_audit.checked_count} checked / ${report.wasm_allocation_audit.excluded_count} excluded | ${report.wasm_allocation_audit.total_wasm_alloc_count} | ${report.wasm_allocation_audit.total_wasm_alloc_bytes} | ${report.wasm_allocation_audit.total_wasm_realloc_count} | ${report.wasm_allocation_audit.total_wasm_realloc_grow_bytes} | ${report.wasm_allocation_audit.max_wasm_allocs_per_frame.toFixed(3)} | ${report.wasm_allocation_audit.max_wasm_alloc_bytes_per_frame.toFixed(3)} | ${report.wasm_allocation_audit.max_wasm_peak_frame_alloc_bytes} | ${report.wasm_allocation_audit.budget_wasm_allocs_per_frame} | ${report.wasm_allocation_audit.budget_wasm_alloc_bytes_per_frame} |`);
   lines.push("");
   lines.push("### WASM Allocation Invariance");
   lines.push("");
   lines.push("| Check | Status | Reference Row | Rows | Unique Signatures | Shared Allocs | Shared Bytes | Shared Reallocs | Shared Peak Frame Bytes |");
   lines.push("| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.wasm_allocation_invariance.id}\` | \`${report.wasm_allocation_invariance.status}\` | \`${report.wasm_allocation_invariance.reference_row}\` | ${report.wasm_allocation_invariance.checked_count} | ${report.wasm_allocation_invariance.unique_signature_count} | ${report.wasm_allocation_invariance.shared_wasm_alloc_count} | ${report.wasm_allocation_invariance.shared_wasm_alloc_bytes} | ${report.wasm_allocation_invariance.shared_wasm_realloc_count} | ${report.wasm_allocation_invariance.shared_wasm_peak_frame_alloc_bytes} |`);
   lines.push("");
   lines.push("### WASM Allocation Rows");
   lines.push("");
   lines.push("| Row | Frames | Allocs | Bytes | Allocs/Frame | Bytes/Frame | Reallocs | Realloc Grow Bytes | Allocating Frames | Peak Frame Bytes |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   for (let row of report.wasm_allocation_audit.row_details) {
      lines.push(`| \`${row.id}\` | ${row.frames} | ${row.wasm_alloc_count} | ${row.wasm_alloc_bytes} | ${row.wasm_allocs_per_frame.toFixed(3)} | ${row.wasm_alloc_bytes_per_frame.toFixed(3)} | ${row.wasm_realloc_count} | ${row.wasm_realloc_grow_bytes} | ${row.wasm_allocating_frames} | ${row.wasm_peak_frame_alloc_bytes} |`);
   }
   lines.push("");
   lines.push("## Frame Loop WASM Allocation Stages");
   lines.push("");
   lines.push("| Stage | Allocs | Bytes | Reallocs | Realloc Grow Bytes | Peak Frame Bytes |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: |");
   for (let row of report.frame_loop_wasm_allocation_stages.stages) {
      lines.push(`| \`${row.stage}\` | ${row.wasm_alloc_count} | ${row.wasm_alloc_bytes} | ${row.wasm_realloc_count} | ${row.wasm_realloc_grow_bytes} | ${row.wasm_peak_frame_alloc_bytes} |`);
   }
   lines.push("");
   lines.push("## Frame Loop WASM Submit Allocation Stages");
   lines.push("");
   lines.push("| Submit Stage | Allocs | Bytes |");
   lines.push("| --- | ---: | ---: |");
   for (let row of report.frame_loop_wasm_submit_allocation_stages.stages) {
      lines.push(`| \`${row.stage}\` | ${row.wasm_alloc_count} | ${row.wasm_alloc_bytes} |`);
   }
   lines.push("");
   lines.push("## Backend Path Coverage");
   lines.push("");
   lines.push("| Path | Status | Comparison | Rows | Counters |");
   lines.push("| --- | --- | --- | ---: | ---: |");
   for (let path of report.backend_path_coverage.paths) {
      lines.push(`| \`${path.id}\` | \`${path.status}\` | \`${path.comparison}\` | ${path.row_count} | ${path.counter_count} |`);
   }
   lines.push("");
   lines.push("## ID-Mask Summary");
   lines.push("");
   lines.push("| Case | Current p50 ms | Current Passes | Uniform Writes | Uniform Bytes | Uniform Slots | Current Upload Bytes | Vertices | Vertex Bytes |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.id_mask_summary.id}\` | ${report.id_mask_summary.current_p50_ms.toFixed(3)} | ${report.id_mask_summary.current_render_passes} | ${report.id_mask_summary.current_uniform_writes} | ${report.id_mask_summary.current_uniform_bytes} | ${report.id_mask_summary.current_uniform_slots} | ${report.id_mask_summary.current_buffer_upload_bytes} | ${report.id_mask_summary.vertices} | ${report.id_mask_summary.vertex_bytes} |`);
   lines.push("");
   lines.push("## Upload Summary");
   lines.push("");
   lines.push("| Case | Glyph Current p50 ms | Glyph Current Texture Bytes | Glyph Current GPU ns | Atlas Dirty WxH | Image Current p50 ms | Image Current Texture Bytes | Image Current GPU ns | Image Dirty WxH |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.upload_summary.id}\` | ${report.upload_summary.glyph_current_p50_ms.toFixed(3)} | ${report.upload_summary.glyph_current_texture_upload_bytes} | ${report.upload_summary.glyph_current_gpu_timestamp_total_ns} | ${report.upload_summary.atlas_dirty_width}x${report.upload_summary.atlas_dirty_height} | ${report.upload_summary.image_current_p50_ms.toFixed(3)} | ${report.upload_summary.image_current_texture_upload_bytes} | ${report.upload_summary.image_current_gpu_timestamp_total_ns} | ${report.upload_summary.image_dirty_width}x${report.upload_summary.image_dirty_height} |`);
   lines.push("");
   lines.push("## Effect Uniform Summary");
   lines.push("");
   lines.push("| Row | Current p50 ms | Current GPU ns | Current Timestamp Passes | Current Writes | Current Bytes | Current Slots | Current Backdrops | Current Texture Copies | Current Passes | Expected Backdrops |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.effect_uniform_summary.id}\` | ${report.effect_uniform_summary.current_p50_ms.toFixed(3)} | ${report.effect_uniform_summary.current_gpu_timestamp_total_ns} | ${report.effect_uniform_summary.current_gpu_timestamp_passes} | ${report.effect_uniform_summary.current_effect_uniform_writes} | ${report.effect_uniform_summary.current_effect_uniform_bytes} | ${report.effect_uniform_summary.current_effect_uniform_slots} | ${report.effect_uniform_summary.current_backdrop_draws} | ${report.effect_uniform_summary.current_texture_copies} | ${report.effect_uniform_summary.current_render_passes} | ${report.effect_uniform_summary.expected_backdrops} |`);
   lines.push("");
   lines.push("## Backdrop Batch Summary");
   lines.push("");
   lines.push("| Row | Current p50 ms | Current Writes | Current Slots | Current Backdrops | Current Texture Copies | Current Passes | Current Timestamp Passes | Expected Backdrops |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.backdrop_batch_summary.id}\` | ${report.backdrop_batch_summary.current_p50_ms.toFixed(3)} | ${report.backdrop_batch_summary.current_effect_uniform_writes} | ${report.backdrop_batch_summary.current_effect_uniform_slots} | ${report.backdrop_batch_summary.current_backdrop_draws} | ${report.backdrop_batch_summary.current_texture_copies} | ${report.backdrop_batch_summary.current_render_passes} | ${report.backdrop_batch_summary.current_gpu_timestamp_passes} | ${report.backdrop_batch_summary.expected_backdrops} |`);
   lines.push("");
   lines.push("## Scene3D Summary");
   lines.push("");
   lines.push("| Comparison | Reused p50 ms | Recreate p50 ms | Recreate / Reused | Reused Mesh Creates | Recreate Mesh Creates | Reused Buffer Grows | Recreate Buffer Grows | Reused CPU Scratch Grows | Recreate CPU Scratch Grows | Reused CPU Scratch Growth Bytes | Recreate CPU Scratch Growth Bytes | Meshes | Instances |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.scene3d_summary.id}\` | ${report.scene3d_summary.reused_p50_ms.toFixed(3)} | ${report.scene3d_summary.recreate_p50_ms.toFixed(3)} | ${report.scene3d_summary.recreate_over_reused.toFixed(3)} | ${report.scene3d_summary.reused_mesh3d_creates} | ${report.scene3d_summary.recreate_mesh3d_creates} | ${report.scene3d_summary.reused_buffer_grows} | ${report.scene3d_summary.recreate_buffer_grows} | ${report.scene3d_summary.reused_cpu_scratch_grows} | ${report.scene3d_summary.recreate_cpu_scratch_grows} | ${report.scene3d_summary.reused_cpu_scratch_growth_bytes} | ${report.scene3d_summary.recreate_cpu_scratch_growth_bytes} | ${report.scene3d_summary.meshes} | ${report.scene3d_summary.instances} |`);
   lines.push("");
   lines.push("## Scene3D Stress Summary");
   lines.push("");
   lines.push("| Comparison | Reused p50 ms | Recreate p50 ms | Recreate / Reused | Reused Mesh Creates | Recreate Mesh Creates | Reused Buffer Grows | Recreate Buffer Grows | Reused CPU Scratch Grows | Recreate CPU Scratch Grows | Reused CPU Scratch Growth Bytes | Recreate CPU Scratch Growth Bytes | Meshes | Instances |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.scene3d_stress_summary.id}\` | ${report.scene3d_stress_summary.reused_p50_ms.toFixed(3)} | ${report.scene3d_stress_summary.recreate_p50_ms.toFixed(3)} | ${report.scene3d_stress_summary.recreate_over_reused.toFixed(3)} | ${report.scene3d_stress_summary.reused_mesh3d_creates} | ${report.scene3d_stress_summary.recreate_mesh3d_creates} | ${report.scene3d_stress_summary.reused_buffer_grows} | ${report.scene3d_stress_summary.recreate_buffer_grows} | ${report.scene3d_stress_summary.reused_cpu_scratch_grows} | ${report.scene3d_stress_summary.recreate_cpu_scratch_grows} | ${report.scene3d_stress_summary.reused_cpu_scratch_growth_bytes} | ${report.scene3d_stress_summary.recreate_cpu_scratch_growth_bytes} | ${report.scene3d_stress_summary.meshes} | ${report.scene3d_stress_summary.instances} |`);
   lines.push("");
   lines.push("## Mixed Scene Summary");
   lines.push("");
   lines.push("| Case | Current p50 ms | Current Items | Pipeline Binds | Bind Groups | Scissors | Writes | Texture Copies | Passes | Glyph Quads | Image Draws | Image Tiles | Layers | Damage Rects | Backdrops | Visual Effects |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.mixed_summary.id}\` | ${report.mixed_summary.current_p50_ms.toFixed(3)} | ${report.mixed_summary.current_draw_items} | ${report.mixed_summary.current_draw_pipeline_binds} | ${report.mixed_summary.current_draw_bind_group_binds} | ${report.mixed_summary.current_draw_scissor_sets} | ${report.mixed_summary.current_effect_uniform_writes} | ${report.mixed_summary.current_texture_copies} | ${report.mixed_summary.current_render_passes} | ${report.mixed_summary.current_glyph_quads} | ${report.mixed_summary.current_image_draws} | ${report.mixed_summary.image_tiles} | ${report.mixed_summary.current_layer_draws} | ${report.mixed_summary.current_damage_rects} | ${report.mixed_summary.current_backdrop_draws} | ${report.mixed_summary.current_visual_effect_draws} |`);
   lines.push("");
   lines.push("## Layer Effects Summary");
   lines.push("");
   lines.push("| Case | Current p50 ms | Current Items | Pipeline Binds | Bind Groups | Scissors | Writes | Texture Copies | Passes | Glyph Quads | Image Draws | Layers | Damage Rects | Backdrops | Visual Effects | Spinners |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.layer_effects_summary.id}\` | ${report.layer_effects_summary.current_p50_ms.toFixed(3)} | ${report.layer_effects_summary.current_draw_items} | ${report.layer_effects_summary.current_draw_pipeline_binds} | ${report.layer_effects_summary.current_draw_bind_group_binds} | ${report.layer_effects_summary.current_draw_scissor_sets} | ${report.layer_effects_summary.current_effect_uniform_writes} | ${report.layer_effects_summary.current_texture_copies} | ${report.layer_effects_summary.current_render_passes} | ${report.layer_effects_summary.current_glyph_quads} | ${report.layer_effects_summary.current_image_draws} | ${report.layer_effects_summary.current_layer_draws} | ${report.layer_effects_summary.current_damage_rects} | ${report.layer_effects_summary.current_backdrop_draws} | ${report.layer_effects_summary.current_visual_effect_draws} | ${report.layer_effects_summary.current_spinner_draws} |`);
   lines.push("");
   lines.push("## Clean Layer Summary");
   lines.push("");
   lines.push("| Row | Clean p50 ms | Clean Items | Clean Hits | Clean Misses | Clean Skipped | Clean Layer Passes | Clean Render Passes | Clean GPU ns | Glyphs | Image Tiles | Expected Layers | Expected Hits |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.clean_layer_summary.id}\` | ${report.clean_layer_summary.clean_p50_ms.toFixed(3)} | ${report.clean_layer_summary.clean_draw_items} | ${report.clean_layer_summary.clean_layer_cache_hits} | ${report.clean_layer_summary.clean_layer_cache_misses} | ${report.clean_layer_summary.clean_layer_cache_skipped_draws} | ${report.clean_layer_summary.clean_layer_passes} | ${report.clean_layer_summary.clean_render_passes} | ${report.clean_layer_summary.clean_gpu_timestamp_total_ns} | ${report.clean_layer_summary.glyphs} | ${report.clean_layer_summary.image_tiles} | ${report.clean_layer_summary.expected_layers} | ${report.clean_layer_summary.expected_clean_hits} |`);
   lines.push("");
   lines.push("## Command Family Summary");
   lines.push("");
   lines.push("| Row | Current p50 ms | Current Items | Current Pipeline Binds | Current Bind Groups | Current Scissors | Image Meshes | Nine Slices | SDF Glyphs | CameraBg Draws |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.command_family_summary.id}\` | ${report.command_family_summary.current_p50_ms.toFixed(3)} | ${report.command_family_summary.current_draw_items} | ${report.command_family_summary.current_draw_pipeline_binds} | ${report.command_family_summary.current_draw_bind_group_binds} | ${report.command_family_summary.current_draw_scissor_sets} | ${report.command_family_summary.current_image_mesh_draws} | ${report.command_family_summary.current_nine_slice_draws} | ${report.command_family_summary.current_sdf_glyph_quads} | ${report.command_family_summary.current_camera_bg_draws} |`);
   lines.push("");
   lines.push("## Glyph Run Summary");
   lines.push("");
   lines.push("| Row | Current p50 ms | Runs | Glyphs/Run | Items | Glyph Quads | SDF Glyphs | Pipeline Binds | Bind Groups | Scissors |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.glyph_run_summary.id}\` | ${report.glyph_run_summary.current_p50_ms.toFixed(3)} | ${report.glyph_run_summary.expected_glyph_runs} | ${report.glyph_run_summary.expected_glyphs_per_run} | ${report.glyph_run_summary.current_draw_items} | ${report.glyph_run_summary.current_glyph_quads} | ${report.glyph_run_summary.current_sdf_glyph_quads} | ${report.glyph_run_summary.current_draw_pipeline_binds} | ${report.glyph_run_summary.current_draw_bind_group_binds} | ${report.glyph_run_summary.current_draw_scissor_sets} |`);
   lines.push("");
   lines.push("## Neon Marker Summary");
   lines.push("");
   lines.push("| Row | Current p50 ms | Markers | Current Items | Expected Items | Current Solid Tris | Current Pipeline Binds | Current Bind Groups | Current Scissors |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.neon_marker_summary.id}\` | ${report.neon_marker_summary.current_p50_ms.toFixed(3)} | ${report.neon_marker_summary.expected_markers} | ${report.neon_marker_summary.current_draw_items} | ${report.neon_marker_summary.expected_draw_items} | ${report.neon_marker_summary.current_solid_tris} | ${report.neon_marker_summary.current_draw_pipeline_binds} | ${report.neon_marker_summary.current_draw_bind_group_binds} | ${report.neon_marker_summary.current_draw_scissor_sets} |`);
   lines.push("");
   lines.push("## Direct Surface Summary");
   lines.push("");
   lines.push("| Row | Current p50 ms | Current Items | Current Images | Current Render Passes | Current Draw Passes | Current Clear Passes | Current Present Passes | Current GPU ns | Current Timestamp Passes | Expected Items | Expected Images |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.direct_surface_summary.id}\` | ${report.direct_surface_summary.current_p50_ms.toFixed(3)} | ${report.direct_surface_summary.current_draw_items} | ${report.direct_surface_summary.current_image_draws} | ${report.direct_surface_summary.current_render_passes} | ${report.direct_surface_summary.current_draw_passes} | ${report.direct_surface_summary.current_clear_passes} | ${report.direct_surface_summary.current_present_passes} | ${report.direct_surface_summary.current_gpu_timestamp_total_ns} | ${report.direct_surface_summary.current_gpu_timestamp_passes} | ${report.direct_surface_summary.expected_draw_items} | ${report.direct_surface_summary.expected_image_draws} |`);
   lines.push("");
   lines.push("## Pixel Check");
   lines.push("");
   lines.push("| Target | Viewport | Pixdiff | Max Err | MSE | Artifact |");
   lines.push("| --- | --- | ---: | ---: | ---: | --- |");
   lines.push(`| \`${report.pixel_check.target}\` | ${report.pixel_check.width}x${report.pixel_check.height} | ${report.pixel_check.pixdiff} | ${report.pixel_check.max_err} | ${report.pixel_check.mse.toFixed(3)} | \`${report.pixel_check.artifact}\` |`);
   lines.push("");
   lines.push("## Notes");
   lines.push("");
   for (let note of report.notes) {
      lines.push(`- ${note}`);
   }
   return `${lines.join("\n")}\n`;
}

function assertNumber(value, label)
{
   if (!Number.isFinite(value)) {
      throw new Error(`web report contract missing finite ${label}`);
   }
}

function assertBrowserStartup(report)
{
   let summary = report.browser_startup;
   if (!summary || summary.id !== "web.wasm.webgpu.browser_startup") {
      throw new Error("web report contract missing browser startup summary");
   }
   if (summary.source !== "performance.now+node.fs.stat") {
      throw new Error("web report contract has unexpected browser startup source");
   }
   for (let key of [
      "page_start_ms",
      "wasm_init_start_ms",
      "wasm_init_ms",
      "app_init_start_ms",
      "app_init_ms",
      "first_frame_start_ms",
      "first_frame_ms",
      "report_ready_ms",
      "wasm_memory_bytes",
      "package_file_count",
      "package_bytes",
      "wasm_bytes",
      "js_bytes",
      "typescript_bytes",
      "wasm_typescript_bytes",
   ]) {
      assertNumber(summary[key], `browser_startup.${key}`);
      if (summary[key] < 0.0) {
         throw new Error(`web report contract has negative browser startup metric ${key}`);
      }
   }
   for (let key of [
      "wasm_init_ms",
      "app_init_ms",
      "report_ready_ms",
      "wasm_memory_bytes",
      "package_file_count",
      "package_bytes",
      "wasm_bytes",
      "js_bytes",
      "typescript_bytes",
      "wasm_typescript_bytes",
   ]) {
      if (summary[key] <= 0.0) {
         throw new Error(`web report contract missing positive browser startup metric ${key}`);
      }
   }
   if (summary.report_ready_ms < summary.first_frame_start_ms) {
      throw new Error("web report contract has browser startup report-ready time before first frame");
   }
   if (summary.package_root !== "host/web-app/www/pkg") {
      throw new Error("web report contract has unexpected browser package root");
   }
   let files = Array.isArray(summary.files) ? summary.files : [];
   if (files.length !== summary.package_file_count) {
      throw new Error("web report contract browser startup package file count mismatch");
   }
   let totalBytes = 0;
   let byKind = new Map();
   for (let file of files) {
      if (typeof file.kind !== "string" || typeof file.path !== "string") {
         throw new Error("web report contract browser startup package file missing labels");
      }
      assertNumber(file.bytes, `browser_startup.${file.kind}.bytes`);
      if (file.bytes <= 0.0) {
         throw new Error(`web report contract browser startup package file missing bytes ${file.path}`);
      }
      totalBytes += file.bytes;
      byKind.set(file.kind, file.bytes);
   }
   if (
      totalBytes !== summary.package_bytes
      || byKind.get("wasm") !== summary.wasm_bytes
      || byKind.get("js") !== summary.js_bytes
      || byKind.get("typescript") !== summary.typescript_bytes
      || byKind.get("wasm_typescript") !== summary.wasm_typescript_bytes
   ) {
      throw new Error("web report contract browser startup package bytes do not reconcile");
   }
}

function assertGpuTimestampStageBreakdown(report, byId)
{
   let summary = report.gpu_timestamp_stage_breakdown;
   if (!summary || summary.id !== "web.wasm.webgpu.gpu_timestamp_stage_breakdown") {
      throw new Error("web report contract missing GPU timestamp stage breakdown");
   }
   let stages = Array.isArray(summary.stages) ? summary.stages : [];
   let rowDetails = Array.isArray(summary.row_details) ? summary.row_details : [];
   let expectedRows = report.cases.filter(row => row.id !== "web.wasm.webgpu.raf_frame_loop").length;
   if (
      summary.row_count !== expectedRows
      || summary.row_detail_count !== rowDetails.length
      || rowDetails.length !== expectedRows
   ) {
      throw new Error("web report contract has inconsistent GPU timestamp row counts");
   }
   if (summary.stage_count !== GPU_TIMESTAMP_STAGE_FIELDS.length || stages.length !== GPU_TIMESTAMP_STAGE_FIELDS.length) {
      throw new Error("web report contract has inconsistent GPU timestamp stage counts");
   }
   let stageByName = new Map();
   for (let stage of stages) {
      if (typeof stage.stage !== "string" || stageByName.has(stage.stage)) {
         throw new Error("web report contract has invalid GPU timestamp stage row");
      }
      assertNumber(stage.pass_count, `gpu_timestamp_stage_breakdown.${stage.stage}.pass_count`);
      assertNumber(stage.timestamp_ns, `gpu_timestamp_stage_breakdown.${stage.stage}.timestamp_ns`);
      stageByName.set(stage.stage, stage);
   }
   let totalStagePasses = 0;
   let totalStageTimestampNs = 0;
   for (let [stage, passField, timestampField] of GPU_TIMESTAMP_STAGE_FIELDS) {
      let detail = stageByName.get(stage);
      if (!detail || detail.pass_field !== passField || detail.timestamp_field !== timestampField) {
         throw new Error(`web report contract missing GPU timestamp stage ${stage}`);
      }
      totalStagePasses += detail.pass_count;
      totalStageTimestampNs += detail.timestamp_ns;
   }
   let rowIds = new Set();
   let totalRenderPasses = 0;
   let totalTimestampPasses = 0;
   let totalTimestampNs = 0;
   let totalFamilyPasses = 0;
   let totalFamilyTimestampNs = 0;
   for (let detail of rowDetails) {
      let source = byId.get(detail.id);
      if (!source || rowIds.has(detail.id)) {
         throw new Error("web report contract has invalid GPU timestamp row detail");
      }
      rowIds.add(detail.id);
      if (
         detail.render_passes !== source.render_passes
         || detail.gpu_timestamp_passes !== source.gpu_timestamp_passes
         || detail.gpu_timestamp_total_ns !== source.gpu_timestamp_total_ns
      ) {
         throw new Error(`web report contract GPU timestamp row detail mismatch ${detail.id}`);
      }
      let detailStages = Array.isArray(detail.stages) ? detail.stages : [];
      if (detailStages.length !== GPU_TIMESTAMP_STAGE_FIELDS.length) {
         throw new Error(`web report contract GPU timestamp row detail stage count mismatch ${detail.id}`);
      }
      let familyPasses = 0;
      let familyTimestampNs = 0;
      let detailStageByName = new Map(detailStages.map(stage => [stage.stage, stage]));
      for (let [stage, passField, timestampField] of GPU_TIMESTAMP_STAGE_FIELDS) {
         let stageDetail = detailStageByName.get(stage);
         if (!stageDetail) {
            throw new Error(`web report contract GPU timestamp row detail missing stage ${detail.id}.${stage}`);
         }
         assertNumber(stageDetail.pass_count, `gpu_timestamp_stage_breakdown.${detail.id}.${stage}.pass_count`);
         assertNumber(stageDetail.timestamp_ns, `gpu_timestamp_stage_breakdown.${detail.id}.${stage}.timestamp_ns`);
         if (
            stageDetail.pass_count !== source[passField]
            || stageDetail.timestamp_ns !== source[timestampField]
         ) {
            throw new Error(`web report contract GPU timestamp row detail source mismatch ${detail.id}.${stage}`);
         }
         familyPasses += stageDetail.pass_count;
         familyTimestampNs += stageDetail.timestamp_ns;
      }
      if (
         detail.family_passes !== familyPasses
         || detail.family_timestamp_ns !== familyTimestampNs
         || familyPasses !== source.render_passes
         || familyTimestampNs !== source.gpu_timestamp_total_ns
      ) {
         throw new Error(`web report contract GPU timestamp family mismatch ${detail.id}`);
      }
      totalRenderPasses += source.render_passes;
      totalTimestampPasses += source.gpu_timestamp_passes;
      totalTimestampNs += source.gpu_timestamp_total_ns;
      totalFamilyPasses += familyPasses;
      totalFamilyTimestampNs += familyTimestampNs;
   }
   if (
      summary.total_render_passes !== totalRenderPasses
      || summary.total_timestamp_passes !== totalTimestampPasses
      || summary.total_timestamp_ns !== totalTimestampNs
      || summary.total_family_passes !== totalFamilyPasses
      || summary.total_family_timestamp_ns !== totalFamilyTimestampNs
      || summary.total_family_passes !== totalStagePasses
      || summary.total_family_timestamp_ns !== totalStageTimestampNs
   ) {
      throw new Error("web report contract GPU timestamp stage totals do not reconcile");
   }
   if (
      summary.collected_rows !== report.gpu_stage_attribution.collected_rows
      || summary.total_timestamp_passes !== report.gpu_stage_attribution.collected_passes
      || summary.total_timestamp_ns !== report.gpu_stage_attribution.total_ns
   ) {
      throw new Error("web report contract GPU timestamp stage totals do not match attribution summary");
   }
}

function assertWarmResourceChurn(report, byId)
{
   let summary = report.warm_resource_churn;
   if (!summary || summary.id !== "web.wasm.webgpu.warm_resource_churn.current_rows") {
      throw new Error("web report contract missing warm resource churn summary");
   }
   let rows = Array.isArray(summary.rows) ? summary.rows : [];
   let excluded = Array.isArray(summary.excluded) ? summary.excluded : [];
   let rowDetails = Array.isArray(summary.row_details) ? summary.row_details : [];
   if (summary.checked_rows !== rows.length || summary.excluded_rows !== excluded.length) {
      throw new Error("web report contract has inconsistent warm resource churn row counts");
   }
   if (summary.row_detail_count !== rowDetails.length || rowDetails.length !== rows.length) {
      throw new Error("web report contract has inconsistent warm resource churn row detail counts");
   }
   let rowSet = new Set(rows);
   let excludedSet = new Set(excluded);
   let rowDetailById = new Map();
   for (let row of rowDetails) {
      if (typeof row.id !== "string" || !rowSet.has(row.id)) {
         throw new Error("web report contract has unexpected warm resource churn row detail");
      }
      if (rowDetailById.has(row.id)) {
         throw new Error(`web report contract has duplicate warm resource churn row detail ${row.id}`);
      }
      rowDetailById.set(row.id, row);
   }
   for (let id of [
      "web.wasm.webgpu.cpu_submit_throughput",
      "web.wasm.webgpu.id_mask_compositor.current",
      "web.wasm.webgpu.glyph_atlas_upload.current_dirty",
      "web.wasm.webgpu.image_upload.current_dirty",
      "web.wasm.webgpu.effect_uniform.current_batched",
      "web.wasm.webgpu.backdrop_batch.current_coalesced",
      "web.wasm.webgpu.scene3d.reused_mesh",
      "web.wasm.webgpu.scene3d.stress_reused_mesh",
      "web.wasm.webgpu.mixed_text_image_effects",
      "web.wasm.webgpu.layer_damage_effects",
      "web.wasm.webgpu.clean_layer.clean_reuse",
      "web.wasm.webgpu.command_family_matrix",
      "web.wasm.webgpu.glyph_run.current",
      "web.wasm.webgpu.neon_marker.current",
      "web.wasm.webgpu.direct_surface.current",
   ]) {
      if (!rowSet.has(id) || !byId.has(id)) {
         throw new Error(`web report contract warm resource churn missing checked row ${id}`);
      }
      if (!rowDetailById.has(id)) {
         throw new Error(`web report contract warm resource churn missing row detail ${id}`);
      }
   }
   for (let id of WARM_RESOURCE_CHURN_EXCLUDED_IDS) {
      if (!excludedSet.has(id) || !byId.has(id)) {
         throw new Error(`web report contract warm resource churn missing excluded row ${id}`);
      }
   }
   let zeroGrowthFields = Array.isArray(summary.zero_growth_fields) ? summary.zero_growth_fields : [];
   for (let field of WARM_RESOURCE_CHURN_FIELDS) {
      if (!zeroGrowthFields.includes(field)) {
         throw new Error(`web report contract warm resource churn missing field ${field}`);
      }
      let total = summary[`total_${field}`];
      if (!Number.isFinite(total) || total !== 0) {
         let offenders = rows
            .map(id => rowDetailById.get(id))
            .filter(detail => detail && Number(detail[field]) !== 0)
            .map(detail => `${detail.id}:${detail[field]}`)
            .join(",");
         throw new Error(`web report contract found warm resource churn ${field}=${total} rows=${offenders}`);
      }
      for (let id of rows) {
         let detail = rowDetailById.get(id);
         let source = byId.get(id);
         let value = detail[field];
         if (!Number.isFinite(value)) {
            throw new Error(`web report contract missing warm resource churn row detail ${id}.${field}`);
         }
         if (value !== source[field]) {
            throw new Error(`web report contract warm resource churn row detail mismatch ${id}.${field}`);
         }
         if (value !== 0) {
            throw new Error(`web report contract found warm resource churn row ${id}.${field}=${value}`);
         }
      }
   }
}

function assertWasmAllocationAudit(report, byId)
{
   let summary = report.wasm_allocation_audit;
   if (!summary || summary.id !== "web.wasm.webgpu.wasm_allocation_audit.current_rows") {
      throw new Error("web report contract missing WASM allocation audit summary");
   }
   let rows = Array.isArray(summary.rows) ? summary.rows : [];
   let excluded = Array.isArray(summary.excluded) ? summary.excluded : [];
   let rowDetails = Array.isArray(summary.row_details) ? summary.row_details : [];
   if (summary.checked_count !== rows.length || summary.excluded_count !== excluded.length) {
      throw new Error("web report contract has inconsistent WASM allocation row counts");
   }
   if (summary.row_detail_count !== rowDetails.length || rowDetails.length !== rows.length) {
      throw new Error("web report contract has inconsistent WASM allocation row detail counts");
   }
   assertNumber(summary.total_wasm_alloc_count, "wasm_allocation_audit.total_wasm_alloc_count");
   assertNumber(summary.total_wasm_alloc_bytes, "wasm_allocation_audit.total_wasm_alloc_bytes");
   assertNumber(summary.total_wasm_realloc_count, "wasm_allocation_audit.total_wasm_realloc_count");
   assertNumber(
      summary.total_wasm_realloc_grow_bytes,
      "wasm_allocation_audit.total_wasm_realloc_grow_bytes",
   );
   assertNumber(summary.max_wasm_allocs_per_frame, "wasm_allocation_audit.max_wasm_allocs_per_frame");
   assertNumber(
      summary.max_wasm_alloc_bytes_per_frame,
      "wasm_allocation_audit.max_wasm_alloc_bytes_per_frame",
   );
   assertNumber(
      summary.max_wasm_peak_frame_alloc_bytes,
      "wasm_allocation_audit.max_wasm_peak_frame_alloc_bytes",
   );
   assertNumber(summary.budget_wasm_allocs_per_frame, "wasm_allocation_audit.budget_wasm_allocs_per_frame");
   assertNumber(
      summary.budget_wasm_alloc_bytes_per_frame,
      "wasm_allocation_audit.budget_wasm_alloc_bytes_per_frame",
   );
   if (summary.total_wasm_alloc_count <= 0 || summary.total_wasm_alloc_bytes <= 0) {
      throw new Error("web report contract expected measured WASM allocation activity");
   }
   if (summary.total_wasm_realloc_count !== 0 || summary.total_wasm_realloc_grow_bytes !== 0) {
      let offenders = rowDetails
         .filter(row => row.wasm_realloc_count !== 0 || row.wasm_realloc_grow_bytes !== 0)
         .map(row => `${row.id}:${row.wasm_realloc_count}/${row.wasm_realloc_grow_bytes}`);
      throw new Error(`web report contract found current-row WASM reallocations: ${offenders.join(", ")}`);
   }
   if (
      summary.max_wasm_allocs_per_frame > summary.budget_wasm_allocs_per_frame
      || summary.max_wasm_alloc_bytes_per_frame > summary.budget_wasm_alloc_bytes_per_frame
   ) {
      throw new Error("web report contract found WASM allocation budget regression");
   }
   let rowSet = new Set(rows);
   for (let id of [
      "web.wasm.webgpu.cpu_submit_throughput",
      "web.wasm.webgpu.id_mask_compositor.current",
      "web.wasm.webgpu.glyph_atlas_upload.current_dirty",
      "web.wasm.webgpu.image_upload.current_dirty",
      "web.wasm.webgpu.effect_uniform.current_batched",
      "web.wasm.webgpu.backdrop_batch.current_coalesced",
      "web.wasm.webgpu.scene3d.reused_mesh",
      "web.wasm.webgpu.scene3d.stress_reused_mesh",
      "web.wasm.webgpu.mixed_text_image_effects",
      "web.wasm.webgpu.layer_damage_effects",
      "web.wasm.webgpu.clean_layer.clean_reuse",
      "web.wasm.webgpu.command_family_matrix",
      "web.wasm.webgpu.glyph_run.current",
      "web.wasm.webgpu.neon_marker.current",
      "web.wasm.webgpu.direct_surface.current",
   ]) {
      if (!rowSet.has(id)) {
         throw new Error(`web report contract missing WASM allocation row ${id}`);
      }
   }
   for (let detail of rowDetails) {
      let source = byId.get(detail.id);
      if (!source || !rowSet.has(detail.id)) {
         throw new Error("web report contract has unexpected WASM allocation row detail");
      }
      for (let field of [
         "frames",
         "wasm_alloc_count",
         "wasm_alloc_bytes",
         "wasm_allocs_per_frame",
         "wasm_alloc_bytes_per_frame",
         "wasm_dealloc_count",
         "wasm_dealloc_bytes",
         "wasm_realloc_count",
         "wasm_realloc_grow_bytes",
         "wasm_realloc_shrink_bytes",
         "wasm_allocating_frames",
         "wasm_peak_frame_alloc_bytes",
      ]) {
         assertNumber(detail[field], `wasm_allocation_audit.${detail.id}.${field}`);
      }
      if (
         detail.wasm_alloc_count !== source.wasm_alloc_count
         || detail.wasm_alloc_bytes !== source.wasm_alloc_bytes
         || detail.wasm_realloc_count !== source.wasm_realloc_count
         || detail.wasm_realloc_grow_bytes !== source.wasm_realloc_grow_bytes
      ) {
         throw new Error(`web report contract WASM allocation row detail mismatch ${detail.id}`);
      }
      if (detail.wasm_realloc_count !== 0 || detail.wasm_realloc_grow_bytes !== 0) {
         throw new Error(`web report contract found WASM reallocations in ${detail.id}`);
      }
      if (
         detail.wasm_allocs_per_frame > summary.budget_wasm_allocs_per_frame
         || detail.wasm_alloc_bytes_per_frame > summary.budget_wasm_alloc_bytes_per_frame
      ) {
         throw new Error(`web report contract found WASM allocation budget regression in ${detail.id}`);
      }
   }
}

function assertWasmAllocationInvariance(report)
{
   let summary = report.wasm_allocation_invariance;
   let audit = report.wasm_allocation_audit;
   if (
      !summary
      || summary.id !== "web.wasm.webgpu.wasm_allocation_invariance.current_rows"
   ) {
      throw new Error("web report contract missing WASM allocation invariance summary");
   }
   if (!audit || audit.id !== "web.wasm.webgpu.wasm_allocation_audit.current_rows") {
      throw new Error("web report contract cannot validate WASM allocation invariance without audit");
   }
   if (
      !["shared-submit-boundary-profile", "path-specific-allocations"].includes(summary.status)
      || summary.unique_signature_count < 1
      || summary.checked_count !== audit.checked_count
      || summary.reference_row !== "web.wasm.webgpu.cpu_submit_throughput"
   ) {
      throw new Error("web report contract has invalid WASM allocation attribution");
   }
   let signatureRows = Array.isArray(summary.signature_rows) ? summary.signature_rows : [];
   if (signatureRows.length !== summary.unique_signature_count
      || signatureRows.some(signature => !Array.isArray(signature.ids))) {
      throw new Error("web report contract has inconsistent WASM allocation invariance signatures");
   }
   let attributedIds = signatureRows.flatMap(signature => signature.ids);
   if (attributedIds.length !== audit.checked_count || new Set(attributedIds).size !== audit.checked_count) {
      throw new Error("web report contract WASM allocation invariance does not cover every checked row");
   }
   let frame = audit.row_details.find(row => row.id === "web.wasm.webgpu.cpu_submit_throughput");
   if (!frame) {
      throw new Error("web report contract missing frame-loop allocation reference row");
   }
   if (
      summary.shared_wasm_alloc_count !== frame.wasm_alloc_count
      || summary.shared_wasm_alloc_bytes !== frame.wasm_alloc_bytes
      || summary.shared_wasm_realloc_count !== 0
      || summary.shared_wasm_realloc_grow_bytes !== 0
      || summary.shared_wasm_allocating_frames !== frame.wasm_allocating_frames
      || summary.shared_wasm_peak_frame_alloc_bytes !== frame.wasm_peak_frame_alloc_bytes
   ) {
      throw new Error("web report contract WASM allocation invariance reference values are inconsistent");
   }
}

function assertFrameLoopWasmStageAllocation(report, byId)
{
   let summary = report.frame_loop_wasm_allocation_stages;
   if (
      !summary
      || summary.id !== "web.wasm.webgpu.frame_loop_wasm_allocation_stages"
      || summary.row_id !== "web.wasm.webgpu.cpu_submit_throughput"
   ) {
      throw new Error("web report contract missing frame-loop WASM allocation stage summary");
   }
   let frame = byId.get("web.wasm.webgpu.cpu_submit_throughput");
   if (!frame) {
      throw new Error("web report contract missing frame-loop row for stage allocation summary");
   }
   let stages = Array.isArray(summary.stages) ? summary.stages : [];
   if (summary.stage_count !== WASM_FRAME_STAGE_NAMES.length || stages.length !== WASM_FRAME_STAGE_NAMES.length) {
      throw new Error("web report contract has inconsistent frame-loop WASM stage counts");
   }
   assertNumber(summary.total_stage_wasm_alloc_count, "frame_loop_wasm_stages.total_stage_wasm_alloc_count");
   assertNumber(summary.total_stage_wasm_alloc_bytes, "frame_loop_wasm_stages.total_stage_wasm_alloc_bytes");
   assertNumber(summary.total_stage_wasm_realloc_count, "frame_loop_wasm_stages.total_stage_wasm_realloc_count");
   assertNumber(
      summary.total_stage_wasm_realloc_grow_bytes,
      "frame_loop_wasm_stages.total_stage_wasm_realloc_grow_bytes",
   );
   if (
      summary.total_stage_wasm_alloc_count !== frame.wasm_alloc_count
      || summary.total_stage_wasm_alloc_bytes !== frame.wasm_alloc_bytes
      || summary.row_wasm_alloc_count !== frame.wasm_alloc_count
      || summary.row_wasm_alloc_bytes !== frame.wasm_alloc_bytes
   ) {
      throw new Error("web report contract found unattributed frame-loop WASM allocations");
   }
   if (
      summary.total_stage_wasm_realloc_count !== 0
      || summary.total_stage_wasm_realloc_grow_bytes !== 0
      || summary.row_wasm_realloc_count !== 0
   ) {
      throw new Error("web report contract found frame-loop WASM stage reallocations");
   }
   let stageNames = new Set(stages.map(stage => stage.stage));
   for (let name of WASM_FRAME_STAGE_NAMES) {
      if (!stageNames.has(name)) {
         throw new Error(`web report contract missing frame-loop WASM allocation stage ${name}`);
      }
   }
   let totalAllocCount = 0;
   let totalAllocBytes = 0;
   for (let stage of stages) {
      for (let field of [
         "wasm_alloc_count",
         "wasm_alloc_bytes",
         "wasm_realloc_count",
         "wasm_realloc_grow_bytes",
         "wasm_peak_frame_alloc_bytes",
      ]) {
         assertNumber(stage[field], `frame_loop_wasm_stages.${stage.stage}.${field}`);
      }
      if (stage.wasm_realloc_count !== 0 || stage.wasm_realloc_grow_bytes !== 0) {
         throw new Error(`web report contract found WASM reallocations in frame stage ${stage.stage}`);
      }
      totalAllocCount += stage.wasm_alloc_count;
      totalAllocBytes += stage.wasm_alloc_bytes;
   }
   if (
      totalAllocCount !== summary.total_stage_wasm_alloc_count
      || totalAllocBytes !== summary.total_stage_wasm_alloc_bytes
   ) {
      throw new Error("web report contract frame-loop WASM stage totals do not match stage rows");
   }
   if (summary.total_stage_wasm_alloc_count <= 0 || !stageNames.has(summary.dominant_stage)) {
      throw new Error("web report contract missing dominant frame-loop WASM allocation stage");
   }
}

function assertFrameLoopWasmSubmitStageAllocation(report, byId)
{
   let summary = report.frame_loop_wasm_submit_allocation_stages;
   if (
      !summary
      || summary.id !== "web.wasm.webgpu.frame_loop_wasm_submit_allocation_stages"
      || summary.row_id !== "web.wasm.webgpu.cpu_submit_throughput"
   ) {
      throw new Error("web report contract missing frame-loop WASM submit allocation stage summary");
   }
   let frame = byId.get("web.wasm.webgpu.cpu_submit_throughput");
   if (!frame) {
      throw new Error("web report contract missing frame-loop row for submit allocation summary");
   }
   let stages = Array.isArray(summary.stages) ? summary.stages : [];
   if (summary.stage_count !== WASM_SUBMIT_STAGE_NAMES.length || stages.length !== WASM_SUBMIT_STAGE_NAMES.length) {
      throw new Error("web report contract has inconsistent frame-loop WASM submit stage counts");
   }
   assertNumber(summary.total_stage_wasm_alloc_count, "frame_loop_wasm_submit_stages.total_stage_wasm_alloc_count");
   assertNumber(summary.total_stage_wasm_alloc_bytes, "frame_loop_wasm_submit_stages.total_stage_wasm_alloc_bytes");
   assertNumber(summary.row_submit_wasm_alloc_count, "frame_loop_wasm_submit_stages.row_submit_wasm_alloc_count");
   assertNumber(summary.row_submit_wasm_alloc_bytes, "frame_loop_wasm_submit_stages.row_submit_wasm_alloc_bytes");
   assertNumber(summary.row_submit_wasm_realloc_count, "frame_loop_wasm_submit_stages.row_submit_wasm_realloc_count");
   assertNumber(
      summary.row_submit_wasm_realloc_grow_bytes,
      "frame_loop_wasm_submit_stages.row_submit_wasm_realloc_grow_bytes",
   );
   if (
      summary.total_stage_wasm_alloc_count !== frame.submit_total_alloc_count
      || summary.total_stage_wasm_alloc_bytes !== frame.submit_total_alloc_bytes
      || summary.row_submit_wasm_alloc_count !== frame.submit_total_alloc_count
      || summary.row_submit_wasm_alloc_bytes !== frame.submit_total_alloc_bytes
      || summary.frame_stage_submit_wasm_alloc_count !== frame.wasm_stage_submit_alloc_count
      || summary.frame_stage_submit_wasm_alloc_bytes !== frame.wasm_stage_submit_alloc_bytes
   ) {
      throw new Error("web report contract found unattributed frame-loop WASM submit allocations");
   }
   if (
      frame.submit_total_alloc_count !== frame.wasm_stage_submit_alloc_count
      || frame.submit_total_alloc_bytes !== frame.wasm_stage_submit_alloc_bytes
   ) {
      throw new Error("web report contract submit sub-stages do not match parent submit stage");
   }
   if (
      summary.row_submit_wasm_realloc_count !== 0
      || summary.row_submit_wasm_realloc_grow_bytes !== 0
      || frame.submit_total_realloc_count !== 0
      || frame.submit_total_realloc_grow_bytes !== 0
   ) {
      throw new Error("web report contract found frame-loop WASM submit reallocations");
   }
   let stageNames = new Set(stages.map(stage => stage.stage));
   let totalAllocCount = 0;
   let totalAllocBytes = 0;
   for (let name of WASM_SUBMIT_STAGE_NAMES) {
      if (!stageNames.has(name)) {
         throw new Error(`web report contract missing WASM submit allocation stage ${name}`);
      }
   }
   for (let stage of stages) {
      assertNumber(stage.wasm_alloc_count, `frame_loop_wasm_submit_stages.${stage.stage}.wasm_alloc_count`);
      assertNumber(stage.wasm_alloc_bytes, `frame_loop_wasm_submit_stages.${stage.stage}.wasm_alloc_bytes`);
      totalAllocCount += stage.wasm_alloc_count;
      totalAllocBytes += stage.wasm_alloc_bytes;
   }
   if (
      totalAllocCount !== summary.total_stage_wasm_alloc_count
      || totalAllocBytes !== summary.total_stage_wasm_alloc_bytes
   ) {
      throw new Error("web report contract frame-loop WASM submit stage totals do not match stage rows");
   }
   if (summary.total_stage_wasm_alloc_count <= 0 || !stageNames.has(summary.dominant_stage)) {
      throw new Error("web report contract missing dominant frame-loop WASM submit allocation stage");
   }
}

function assertBackendPathCoverage(report, byId)
{
   let summary = report.backend_path_coverage;
   if (!summary || summary.id !== "web.wasm.webgpu.backend_path_coverage") {
      throw new Error("web report contract missing WebGPU backend path coverage");
   }
   if (summary.expected_path_count !== WEBGPU_BACKEND_PATHS.length) {
      throw new Error("web report contract backend path expected count mismatch");
   }
   if (summary.covered_path_count !== WEBGPU_BACKEND_PATHS.length || summary.missing_path_count !== 0) {
      throw new Error("web report contract has uncovered WebGPU backend paths");
   }
   let paths = Array.isArray(summary.paths) ? summary.paths : [];
   if (paths.length !== WEBGPU_BACKEND_PATHS.length) {
      throw new Error("web report contract backend path row count mismatch");
   }
   let pathById = new Map(paths.map(path => [path.id, path]));
   for (let spec of WEBGPU_BACKEND_PATHS) {
      let path = pathById.get(spec.id);
      if (!path) {
         throw new Error(`web report contract missing backend path ${spec.id}`);
      }
      if (path.status !== "covered" || path.comparison !== spec.comparison) {
         throw new Error(`web report contract backend path ${spec.id} is not covered`);
      }
      if (path.row_count !== spec.rows.length || path.counter_count !== spec.counters.length) {
         throw new Error(`web report contract backend path ${spec.id} count mismatch`);
      }
      let rows = Array.isArray(path.rows) ? path.rows : [];
      let counters = Array.isArray(path.counters) ? path.counters : [];
      let rowDetails = Array.isArray(path.row_details) ? path.row_details : [];
      if (rowDetails.length !== spec.rows.length) {
         throw new Error(`web report contract backend path ${spec.id} row detail mismatch`);
      }
      if ((path.missing_rows || []).length !== 0 || (path.missing_counters || []).length !== 0) {
         throw new Error(`web report contract backend path ${spec.id} has missing evidence`);
      }
      for (let rowId of spec.rows) {
         if (!rows.includes(rowId) || !byId.has(rowId)) {
            throw new Error(`web report contract backend path ${spec.id} missing row ${rowId}`);
         }
      }
      for (let field of spec.counters) {
         if (!counters.includes(field)) {
            throw new Error(`web report contract backend path ${spec.id} missing counter ${field}`);
         }
      }
      let rowDetailById = new Map(rowDetails.map(row => [row.id, row]));
      for (let rowId of spec.rows) {
         let row = byId.get(rowId);
         let detail = rowDetailById.get(rowId);
         if (!detail) {
            throw new Error(`web report contract backend path ${spec.id} missing row detail ${rowId}`);
         }
         for (let field of ["p50_ms", "p95_ms", "p99_ms", "peak_ms"]) {
            assertNumber(detail[field], `backend_path_coverage.${spec.id}.${rowId}.${field}`);
            let rowValue = row[field] ?? row[`cpu_submit_${field}`];
            if (detail[field] !== rowValue) {
               throw new Error(`web report contract backend path ${spec.id} distribution mismatch ${rowId}.${field}`);
            }
         }
         if (detail.p50_ms <= 0.0 || detail.p95_ms < detail.p50_ms || detail.p99_ms < detail.p95_ms || detail.peak_ms < detail.p99_ms) {
            throw new Error(`web report contract backend path ${spec.id} invalid distribution for ${rowId}`);
         }
         if (!detail.counters || typeof detail.counters !== "object") {
            throw new Error(`web report contract backend path ${spec.id} missing counters for ${rowId}`);
         }
         for (let field of spec.counters) {
            let value = detail.counters[field];
            assertNumber(value, `backend_path_coverage.${spec.id}.${rowId}.${field}`);
            if (value !== row[field]) {
               throw new Error(`web report contract backend path ${spec.id} counter mismatch ${rowId}.${field}`);
            }
         }
      }
   }
}

function assertBenchmarkMarks(report)
{
   let summary = report.benchmark_marks;
   if (!summary || summary.id !== "web.wasm.webgpu.benchmark_mark_coverage") {
      throw new Error("web report contract missing benchmark mark coverage");
   }
   if (summary.expected_count !== EXPECTED_BENCHMARK_MARKS.length) {
      throw new Error("web report contract benchmark mark expected count mismatch");
   }
   let pageLabels = Array.isArray(summary.page_labels) ? summary.page_labels : [];
   let tracedLabels = Array.isArray(summary.traced_labels) ? summary.traced_labels : [];
   for (let id of EXPECTED_BENCHMARK_MARKS) {
      if (!pageLabels.includes(id)) {
         throw new Error(`web report contract missing page benchmark mark ${id}`);
      }
   }
   if (summary.page_mark_count !== pageLabels.length || summary.page_mark_count < EXPECTED_BENCHMARK_MARKS.length) {
      throw new Error("web report contract benchmark mark page count mismatch");
   }
   assertNumber(summary.js_heap_sample_supported_count, "benchmark_marks.js_heap_sample_supported_count");
   assertNumber(summary.js_heap_gc_available_count, "benchmark_marks.js_heap_gc_available_count");
   assertNumber(summary.js_heap_total_growth_bytes, "benchmark_marks.js_heap_total_growth_bytes");
   assertNumber(summary.js_heap_max_growth_bytes, "benchmark_marks.js_heap_max_growth_bytes");
   if (!Array.isArray(summary.js_heap_growth_labels)) {
      throw new Error("web report contract benchmark mark missing JS heap growth labels");
   }
   if (
      summary.js_heap_sample_supported_count < EXPECTED_BENCHMARK_MARKS.length
      || summary.js_heap_gc_available_count < EXPECTED_BENCHMARK_MARKS.length
   ) {
      throw new Error("web report contract benchmark marks require Chrome JS heap sampling and exposed GC");
   }
   assertNumber(summary.wasm_memory_total_growth_bytes, "benchmark_marks.wasm_memory_total_growth_bytes");
   assertNumber(summary.wasm_memory_max_growth_bytes, "benchmark_marks.wasm_memory_max_growth_bytes");
   if (!Array.isArray(summary.wasm_memory_growth_labels)) {
      throw new Error("web report contract benchmark mark missing wasm memory growth labels");
   }
   if (
      summary.wasm_memory_total_growth_bytes !== 0
      || summary.wasm_memory_max_growth_bytes !== 0
      || summary.wasm_memory_growth_labels.length !== 0
   ) {
      throw new Error("web report contract benchmark marks must have zero WASM memory growth after prewarm");
   }
   let marks = Array.isArray(summary.marks) ? summary.marks : [];
   if (marks.length !== summary.page_mark_count) {
      throw new Error("web report contract benchmark mark array count mismatch");
   }
   for (let mark of marks) {
      if (!pageLabels.includes(mark.id)) {
         throw new Error(`web report contract benchmark mark label mismatch ${mark.id}`);
      }
      assertNumber(mark.start_ms, `${mark.id}.start_ms`);
      assertNumber(mark.duration_ms, `${mark.id}.duration_ms`);
      if (mark.start_ms < 0.0 || mark.duration_ms <= 0.0) {
         throw new Error(`web report contract benchmark mark has invalid timing ${mark.id}`);
      }
      assertNumber(mark.wasm_memory_before_bytes, `${mark.id}.wasm_memory_before_bytes`);
      assertNumber(mark.wasm_memory_after_bytes, `${mark.id}.wasm_memory_after_bytes`);
      assertNumber(mark.wasm_memory_growth_bytes, `${mark.id}.wasm_memory_growth_bytes`);
      assertNumber(mark.js_heap_sample_supported, `${mark.id}.js_heap_sample_supported`);
      assertNumber(mark.js_heap_gc_available, `${mark.id}.js_heap_gc_available`);
      assertNumber(mark.js_heap_before_bytes, `${mark.id}.js_heap_before_bytes`);
      assertNumber(mark.js_heap_after_bytes, `${mark.id}.js_heap_after_bytes`);
      assertNumber(mark.js_heap_growth_bytes, `${mark.id}.js_heap_growth_bytes`);
      if (
         mark.wasm_memory_before_bytes <= 0.0
         || mark.wasm_memory_after_bytes < mark.wasm_memory_before_bytes
         || mark.wasm_memory_growth_bytes < 0.0
      ) {
         throw new Error(`web report contract benchmark mark has invalid wasm memory fields ${mark.id}`);
      }
      if (
         mark.wasm_memory_after_bytes !== mark.wasm_memory_before_bytes
         || mark.wasm_memory_growth_bytes !== 0
      ) {
         throw new Error(`web report contract benchmark mark grew WASM memory after prewarm ${mark.id}`);
      }
      if (
         mark.js_heap_sample_supported <= 0.0
         || mark.js_heap_gc_available <= 0.0
         || mark.js_heap_before_bytes <= 0.0
         || mark.js_heap_after_bytes <= 0.0
         || mark.js_heap_growth_bytes < 0.0
      ) {
         throw new Error(`web report contract benchmark mark has invalid JS heap fields ${mark.id}`);
      }
   }
   if (report.browser_trace.benchmark_trace_mark_status === "collected") {
      for (let id of EXPECTED_BENCHMARK_MARKS) {
         if (!tracedLabels.includes(id)) {
            throw new Error(`web report contract missing traced benchmark mark ${id}`);
         }
      }
      if (summary.traced_mark_count !== EXPECTED_BENCHMARK_MARKS.length) {
         throw new Error("web report contract traced benchmark mark count mismatch");
      }
   }
}

function assertWebReportContract(report)
{
   let expected = new Set([
      "web.wasm.webgpu.cpu_submit_throughput",
      "web.wasm.webgpu.raf_frame_loop",
      "web.wasm.webgpu.id_mask_compositor.current",
      "web.wasm.webgpu.glyph_atlas_upload.current_dirty",
      "web.wasm.webgpu.image_upload.current_dirty",
      "web.wasm.webgpu.effect_uniform.current_batched",
      "web.wasm.webgpu.backdrop_batch.current_coalesced",
      "web.wasm.webgpu.scene3d.reused_mesh",
      "web.wasm.webgpu.scene3d.recreate_mesh",
      "web.wasm.webgpu.scene3d.stress_reused_mesh",
      "web.wasm.webgpu.scene3d.stress_recreate_mesh",
      "web.wasm.webgpu.mixed_text_image_effects",
      "web.wasm.webgpu.layer_damage_effects",
      "web.wasm.webgpu.clean_layer.clean_reuse",
      "web.wasm.webgpu.command_family_matrix",
      "web.wasm.webgpu.glyph_run.current",
      "web.wasm.webgpu.neon_marker.current",
      "web.wasm.webgpu.direct_surface.current",
   ]);
   let cpuScratchGrowthAllowed = new Set([
      "web.wasm.webgpu.scene3d.recreate_mesh",
      "web.wasm.webgpu.scene3d.stress_recreate_mesh",
   ]);
   for (let row of report.cases) {
      expected.delete(row.id);
      if (row.id === "web.wasm.webgpu.cpu_submit_throughput") {
         for (let key of [
            "samples",
            "frames_per_sample",
            "frames",
            "cpu_submit_p50_ms",
            "cpu_submit_p95_ms",
            "cpu_submit_p99_ms",
            "cpu_submit_peak_ms",
            "cpu_submit_avg_ms",
         ]) {
            assertNumber(row[key], `${row.id}.${key}`);
         }
         for (let key of [
            "p50_ms",
            "p95_ms",
            "p99_ms",
            "peak_ms",
            "missed_frames_60hz",
            "hitch_frames_60hz",
            "missed_frames_120hz",
            "hitch_frames_120hz",
         ]) {
            if (Object.hasOwn(row, key)) {
               throw new Error(`CPU-submit throughput row must not claim displayed-frame metric ${key}`);
            }
         }
         continue;
      }
      if (row.id === "web.wasm.webgpu.raf_frame_loop") {
         for (let key of [
            "samples",
            "frames",
            "submissions",
            "p50_ms",
            "p95_ms",
            "p99_ms",
            "peak_ms",
            "cpu_submit_p50_ms",
            "cpu_submit_p95_ms",
            "cpu_submit_p99_ms",
            "cpu_submit_peak_ms",
            "queue_drain_ms",
            "queue_drain_raf_waits",
            "queue_pending_initial",
            "queue_pending_final",
            "missed_frames_60hz",
            "hitch_frames_60hz",
            "missed_frames_120hz",
            "hitch_frames_120hz",
         ]) {
            assertNumber(row[key], `${row.id}.${key}`);
         }
         if (row.frames < 2000
            || row.submissions !== row.frames
            || row.raf_deltas_ms.length !== row.frames
            || row.raf_timestamps_ms.length !== row.frames
            || row.cpu_submit_ms.length !== row.frames
            || row.warmup_cpu_submit_ms.length !== row.warmup_frames
            || row.instrumentation_enabled_ms.length !== 200
            || row.instrumentation_disabled_ms.length !== 200
            || row.submissions_per_raf !== 1
            || row.production_coalescing !== "adjacent-order-preserving"
            || row.production_damage_policy !== "router-damage-handoff"
            || !row.cross_origin_isolated
            || row.production_path !== 1) {
            throw new Error("RAF row violates raw displayed-frame cardinality or production-path contract");
         }
         if (row.gpu_timestamp_status === "collected") {
            if (row.gpu_timestamp_samples.length < 2000 || row.gpu_ms.length < 2000) {
               throw new Error("RAF row has fewer than 2000 GPU timestamp samples");
            }
            if (row.queue_pending_final !== 0) {
               throw new Error("RAF row retained pending GPU timestamp readbacks after queue drain");
            }
            for (let key of ["gpu_ms_p50", "gpu_ms_p95", "gpu_ms_p99", "gpu_ms_peak"]) {
               assertNumber(row[key], `${row.id}.${key}`);
            }
         } else if (row.gpu_timestamp_status !== "unsupported") {
            throw new Error("RAF row has an invalid GPU timestamp status");
         }
         continue;
      }
      for (let key of [
         "samples",
         "frames_per_sample",
         "frames",
         "p50_ms",
         "p95_ms",
         "p99_ms",
         "peak_ms",
         "avg_ms",
         "draws",
         "draw_items",
         "draw_items_coalesced",
         "draw_pipeline_binds",
         "draw_bind_group_binds",
         "draw_scissor_sets",
         "solid_tris",
         "image_draws",
         "image_mesh_draws",
         "nine_slice_draws",
         "glyph_quads",
         "sdf_glyph_quads",
         "clip_depth_peak",
         "damage_rects",
         "layer_draws",
         "layer_cache_hits",
         "layer_cache_misses",
         "layer_cache_skipped_draws",
         "layer_passes",
         "scene3d_draws",
         "id_mask_draws",
         "backdrop_draws",
         "visual_effect_draws",
         "effect_uniform_writes",
         "effect_uniform_bytes",
         "effect_uniform_slots",
         "spinner_draws",
         "camera_bg_draws",
         "render_passes",
         "clear_passes",
         "draw_passes",
         "scene3d_passes",
         "scene3d_overlay_passes",
         "id_mask_raster_passes",
         "id_mask_field_seed_passes",
         "id_mask_field_jump_passes",
         "id_mask_compositor_passes",
         "present_passes",
         "texture_copies",
         "command_buffers",
         "gpu_timestamp_query_supported",
         "gpu_timestamp_frame_id",
         "gpu_timestamp_passes",
         "gpu_timestamp_total_ns",
         "gpu_timestamp_clear_ns",
         "gpu_timestamp_draw_ns",
         "gpu_timestamp_scene3d_ns",
         "gpu_timestamp_scene3d_overlay_ns",
         "gpu_timestamp_id_mask_raster_ns",
         "gpu_timestamp_id_mask_field_seed_ns",
         "gpu_timestamp_id_mask_field_jump_ns",
         "gpu_timestamp_id_mask_compositor_ns",
         "gpu_timestamp_present_ns",
         "gpu_timestamp_max_pass_ns",
         "gpu_timestamp_readback_skips",
         "gpu_timestamp_readback_interval",
         "buffer_upload_bytes",
         "texture_upload_bytes",
         "buffer_grows",
         "texture_creates",
         "bind_group_creates",
         "pipeline_creates",
         "sampler_creates",
         "mesh3d_creates",
         "draw_buffer_grows",
         "image_texture_creates",
         "image_bind_group_creates",
         "target_texture_creates",
         "target_bind_group_creates",
         "layer_texture_creates",
         "layer_bind_group_creates",
         "scene3d_buffer_grows",
         "scene3d_bind_group_creates",
         "effect_buffer_grows",
         "effect_bind_group_creates",
         "id_mask_texture_creates",
         "id_mask_buffer_grows",
         "id_mask_bind_group_creates",
         "image_upload_temp_allocs",
         "image_upload_temp_bytes",
         "image_upload_scratch_bytes",
         "image_upload_scratch_grows",
         "cpu_scratch_bytes",
         "cpu_scratch_grows",
         "cpu_scratch_growth_bytes",
         "cpu_draw_scratch_bytes",
         "cpu_draw_scratch_grows",
         "cpu_draw_scratch_growth_bytes",
         "cpu_scene3d_scratch_bytes",
         "cpu_scene3d_scratch_grows",
         "cpu_scene3d_scratch_growth_bytes",
         "cpu_effect_scratch_bytes",
         "cpu_effect_scratch_grows",
         "cpu_effect_scratch_growth_bytes",
         "cpu_id_mask_scratch_bytes",
         "cpu_id_mask_scratch_grows",
         "cpu_id_mask_scratch_growth_bytes",
         "cpu_image_upload_scratch_bytes",
         "cpu_image_upload_scratch_grows",
         "cpu_image_upload_scratch_growth_bytes",
         "cpu_resource_table_scratch_bytes",
         "cpu_resource_table_scratch_grows",
         "cpu_resource_table_scratch_growth_bytes",
      ]) {
         assertNumber(row[key], `${row.id}.${key}`);
      }
      if (row.samples <= 0 || row.frames_per_sample <= 0 || row.frames <= 0) {
         throw new Error(`web report contract invalid sample counts for ${row.id}`);
      }
      if (row.p50_ms <= 0.0 || row.p95_ms < row.p50_ms || row.p99_ms < row.p95_ms || row.peak_ms < row.p99_ms) {
         throw new Error(`web report contract invalid latency distribution for ${row.id}`);
      }
      if (row.command_buffers <= 0 || row.render_passes <= 0) {
         throw new Error(`web report contract missing command/render-pass work for ${row.id}`);
      }
      let passFamilyTotal =
         row.clear_passes
         + row.draw_passes
         + row.scene3d_passes
         + row.scene3d_overlay_passes
         + row.id_mask_raster_passes
         + row.id_mask_field_seed_passes
         + row.id_mask_field_jump_passes
         + row.id_mask_compositor_passes
         + row.present_passes;
      if (passFamilyTotal !== row.render_passes) {
         throw new Error(`web report contract pass-family total ${passFamilyTotal} != render_passes ${row.render_passes} for ${row.id}`);
      }
      if (row.gpu_timestamp_query_supported > 0 && row.gpu_timestamp_passes > 0 && row.gpu_timestamp_passes !== row.render_passes) {
         throw new Error(`web report contract timestamp passes ${row.gpu_timestamp_passes} != render_passes ${row.render_passes} for ${row.id}`);
      }
      if (row.gpu_timestamp_query_supported > 0 && row.gpu_timestamp_readback_interval < 1) {
         throw new Error(`web report contract missing timestamp readback interval for ${row.id}`);
      }
      if (
         row.pipeline_creates !== 0
         || row.bind_group_creates !== 0
         || row.texture_creates !== 0
         || row.sampler_creates !== 0
      ) {
         throw new Error(`web report contract found post-warmup resource creation in ${row.id}`);
      }
      if (row.bind_group_creates !== row.image_bind_group_creates + row.target_bind_group_creates + row.scene3d_bind_group_creates + row.effect_bind_group_creates + row.id_mask_bind_group_creates) {
         throw new Error(`web report contract bind-group family mismatch in ${row.id}`);
      }
      if (row.texture_creates !== row.image_texture_creates + row.target_texture_creates + row.id_mask_texture_creates) {
         throw new Error(`web report contract texture family mismatch in ${row.id}`);
      }
      if (row.buffer_grows !== row.draw_buffer_grows + row.scene3d_buffer_grows + row.effect_buffer_grows + row.id_mask_buffer_grows) {
         throw new Error(`web report contract buffer family mismatch in ${row.id}`);
      }
      if (
         row.id !== "web.wasm.webgpu.scene3d.recreate_mesh"
         && row.id !== "web.wasm.webgpu.scene3d.stress_recreate_mesh"
         && row.buffer_grows !== 0
      ) {
         throw new Error(`web report contract found post-warmup buffer growth in ${row.id}`);
      }
      if (row.cpu_scratch_bytes <= 0) {
         throw new Error(`web report contract missing CPU scratch capacity in ${row.id}`);
      }
      if (row.cpu_draw_scratch_bytes <= 0 || row.cpu_resource_table_scratch_bytes <= 0) {
         throw new Error(`web report contract missing family CPU scratch capacity in ${row.id}`);
      }
      if (
         !cpuScratchGrowthAllowed.has(row.id)
         && (row.cpu_scratch_grows !== 0 || row.cpu_scratch_growth_bytes !== 0)
      ) {
         throw new Error(`web report contract found post-warmup CPU scratch growth in ${row.id}`);
      }
      if (!cpuScratchGrowthAllowed.has(row.id)) {
         for (let field of WARM_RESOURCE_CHURN_FIELDS) {
            if (field.startsWith("cpu_") && row[field] !== 0) {
               throw new Error(`web report contract found post-warmup family CPU scratch growth in ${row.id}.${field}`);
            }
            if (
               (
                  field.endsWith("_buffer_grows")
                  || field.endsWith("_texture_creates")
                  || field.endsWith("_bind_group_creates")
               )
               && row[field] !== 0
            ) {
               throw new Error(`web report contract found post-warmup family GPU resource churn in ${row.id}.${field}`);
            }
         }
      }
   }
   if (expected.size > 0) {
      throw new Error(`web report contract missing cases: ${[...expected].join(", ")}`);
   }
   if (!report.gpu_stage_attribution || typeof report.gpu_stage_attribution.status !== "string") {
      throw new Error("web report contract missing GPU stage attribution status");
   }
   let collectedRows = report.cases.filter(row => row.gpu_timestamp_passes > 0);
   if (
      report.gpu_stage_attribution.source !== "adapter.features"
      && report.gpu_stage_attribution.source !== "adapter.features+renderer.timestamp_writes"
   ) {
      throw new Error("web report contract has unexpected GPU stage attribution source");
   }
   if (report.gpu_stage_attribution.timestamp_query === "adapter-supported") {
      if (collectedRows.length <= 0 || report.gpu_stage_attribution.status !== "timestamp-query-collected") {
         throw new Error("web report contract requires collected timestamp-query rows on supported adapters");
      }
   }
   if (report.gpu_stage_attribution.status === "timestamp-query-collected") {
      if (report.gpu_stage_attribution.collected_rows !== collectedRows.length) {
         throw new Error("web report contract has inconsistent collected timestamp row count");
      }
      if (report.gpu_stage_attribution.collected_passes <= 0) {
         throw new Error("web report contract missing collected timestamp passes");
      }
   }
   if (!report.browser_trace || report.browser_trace.status !== "collected") {
      throw new Error("web report contract missing collected Chrome browser trace");
   }
   if (report.browser_trace.capture_phase !== "benchmark-report") {
      throw new Error("web report contract requires Chrome trace from the benchmark-report run");
   }
   if (report.browser_trace.timing_source !== "untraced-baseline-report") {
      throw new Error("web report contract requires untraced timing rows with traced duplicate report evidence");
   }
   for (let key of ["events", "gpu_related_events", "duration_us", "category_count"]) {
      assertNumber(report.browser_trace[key], `browser_trace.${key}`);
      if (report.browser_trace[key] <= 0) {
         throw new Error(`web report contract missing positive browser trace ${key}`);
      }
   }
   if (!Array.isArray(report.browser_trace.sample_categories) || report.browser_trace.sample_categories.length <= 0) {
      throw new Error("web report contract missing browser trace categories");
   }
   if (typeof report.browser_trace.benchmark_trace_mark_status !== "string") {
      throw new Error("web report contract missing browser trace benchmark mark status");
   }
   if (report.browser_trace.benchmark_trace_mark_status !== "collected") {
      throw new Error("web report contract requires benchmark User Timing marks in the Chrome trace");
   }
   assertNumber(report.browser_trace.benchmark_trace_mark_count, "browser_trace.benchmark_trace_mark_count");
   if (report.browser_trace.benchmark_trace_mark_count <= 0) {
      throw new Error("web report contract missing browser trace benchmark mark events");
   }
   if (!Array.isArray(report.browser_trace.benchmark_trace_mark_labels)) {
      throw new Error("web report contract missing browser trace benchmark mark labels");
   }
   if (!Array.isArray(report.browser_trace.benchmark_trace_marks)) {
      throw new Error("web report contract missing browser trace benchmark marks");
   }
   assertNumber(report.browser_trace.benchmark_trace_interval_count, "browser_trace.benchmark_trace_interval_count");
   if (report.browser_trace.benchmark_trace_interval_count !== EXPECTED_BENCHMARK_MARKS.length) {
      throw new Error("web report contract requires one browser trace interval per benchmark family");
   }
   if (!Array.isArray(report.browser_trace.benchmark_trace_interval_labels)) {
      throw new Error("web report contract missing browser trace benchmark interval labels");
   }
   if (!Array.isArray(report.browser_trace.benchmark_trace_intervals)) {
      throw new Error("web report contract missing browser trace benchmark intervals");
   }
   for (let id of EXPECTED_BENCHMARK_MARKS) {
      if (!report.browser_trace.benchmark_trace_interval_labels.includes(id)) {
         throw new Error(`web report contract missing browser trace benchmark interval ${id}`);
      }
   }
   for (let interval of report.browser_trace.benchmark_trace_intervals) {
      if (!EXPECTED_BENCHMARK_MARKS.includes(interval.id)) {
         throw new Error(`web report contract has unexpected browser trace benchmark interval ${interval.id}`);
      }
      for (let key of ["duration_us", "event_count", "gpu_related_events", "webgpu_related_events", "event_duration_us"]) {
         assertNumber(interval[key], `browser_trace.${interval.id}.${key}`);
         if (interval[key] <= 0) {
            throw new Error(`web report contract missing positive browser trace benchmark interval ${interval.id}.${key}`);
         }
      }
      for (let key of ["angle_related_events", "renderer_related_events"]) {
         assertNumber(interval[key], `browser_trace.${interval.id}.${key}`);
         if (interval[key] < 0) {
            throw new Error(`web report contract has invalid browser trace benchmark interval ${interval.id}.${key}`);
         }
	      }
	   }
	   assertBrowserStartup(report);
	   assertBenchmarkMarks(report);
	   let byId = new Map(report.cases.map(row => [row.id, row]));
   assertGpuTimestampStageBreakdown(report, byId);
   assertWarmResourceChurn(report, byId);
   assertWasmAllocationAudit(report, byId);
   assertWasmAllocationInvariance(report);
   assertFrameLoopWasmStageAllocation(report, byId);
   assertFrameLoopWasmSubmitStageAllocation(report, byId);
   assertBackendPathCoverage(report, byId);
   if (byId.get("web.wasm.webgpu.id_mask_compositor.current").id_mask_draws <= 0) {
      throw new Error("web report contract missing ID-mask draw counter");
   }
   if (byId.get("web.wasm.webgpu.scene3d.reused_mesh").scene3d_draws <= 0) {
      throw new Error("web report contract missing Scene3D reused draw counter");
   }
   if (byId.get("web.wasm.webgpu.scene3d.reused_mesh").mesh3d_creates !== 0) {
      throw new Error("reused Scene3D row must not create meshes after warmup");
   }
   if (byId.get("web.wasm.webgpu.scene3d.recreate_mesh").mesh3d_creates <= 0) {
      throw new Error("recreate Scene3D row must expose mesh churn");
   }
   if (byId.get("web.wasm.webgpu.scene3d.stress_reused_mesh").scene3d_draws < 64) {
      throw new Error("stress Scene3D reused row must cover many Scene3D draws");
   }
   if (byId.get("web.wasm.webgpu.scene3d.stress_reused_mesh").mesh3d_creates !== 0) {
      throw new Error("stress reused Scene3D row must not create meshes after warmup");
   }
   if (byId.get("web.wasm.webgpu.scene3d.stress_recreate_mesh").mesh3d_creates <= 0) {
      throw new Error("stress recreate Scene3D row must expose mesh churn");
   }
   if (byId.get("web.wasm.webgpu.scene3d.stress_recreate_mesh").scene3d_draws < 64) {
      throw new Error("stress recreate Scene3D row must cover many Scene3D draws");
   }
   let mixed = byId.get("web.wasm.webgpu.mixed_text_image_effects");
   if (byId.has("web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched")) {
      throw new Error("mixed WebGPU legacy rebind row was retired from the default browser report");
   }
   if (
      mixed.backdrop_draws <= 0
      || mixed.visual_effect_draws <= 0
      || mixed.layer_draws <= 0
      || mixed.damage_rects <= 0
      || mixed.clip_depth_peak <= 0
      || mixed.glyph_quads <= 0
      || mixed.image_draws < mixed.image_tiles
      || mixed.spinner_draws <= 0
      || mixed.effect_uniform_writes <= 0
      || mixed.texture_copies <= 0
      || mixed.render_passes <= 1
      || mixed.gpu_timestamp_passes !== mixed.render_passes
   ) {
      throw new Error(
         "mixed WebGPU current row must cover glyph/image/effect/layer/clip/damage work and timestamped passes: "
            + `glyphs=${mixed.glyph_quads} `
            + `images=${mixed.image_draws} tiles=${mixed.image_tiles} `
            + `backdrops=${mixed.backdrop_draws} `
            + `visual_effects=${mixed.visual_effect_draws} `
            + `layers=${mixed.layer_draws} `
            + `damage=${mixed.damage_rects} `
            + `clip_peak=${mixed.clip_depth_peak} `
            + `effect_writes=${mixed.effect_uniform_writes} `
            + `copies=${mixed.texture_copies} `
            + `passes=${mixed.render_passes} `
            + `timestamp_passes=${mixed.gpu_timestamp_passes}`
      );
   }
   if (
      report.mixed_summary.current_p50_ms !== mixed.p50_ms
      || report.mixed_summary.current_draw_pipeline_binds !== mixed.draw_pipeline_binds
      || report.mixed_summary.current_draw_bind_group_binds !== mixed.draw_bind_group_binds
      || report.mixed_summary.current_draw_scissor_sets !== mixed.draw_scissor_sets
      || report.mixed_summary.current_effect_uniform_writes !== mixed.effect_uniform_writes
      || report.mixed_summary.current_texture_copies !== mixed.texture_copies
      || report.mixed_summary.current_render_passes !== mixed.render_passes
      || report.mixed_summary.current_glyph_quads !== mixed.glyph_quads
      || report.mixed_summary.current_image_draws !== mixed.image_draws
      || report.mixed_summary.current_backdrop_draws !== mixed.backdrop_draws
      || report.mixed_summary.current_visual_effect_draws !== mixed.visual_effect_draws
      || report.mixed_summary.current_layer_draws !== mixed.layer_draws
      || report.mixed_summary.current_damage_rects !== mixed.damage_rects
   ) {
      throw new Error("mixed WebGPU summary must match the current source row");
   }
   let layerEffects = byId.get("web.wasm.webgpu.layer_damage_effects");
   if (
      layerEffects.layer_draws < layerEffects.expected_layers
      || layerEffects.damage_rects < layerEffects.expected_damage_rects
      || layerEffects.clip_depth_peak <= 0
      || layerEffects.backdrop_draws < layerEffects.expected_backdrops
      || layerEffects.visual_effect_draws <= 0
      || layerEffects.spinner_draws <= 0
      || layerEffects.texture_copies <= 0
      || layerEffects.gpu_timestamp_passes !== layerEffects.render_passes
   ) {
      throw new Error("layer/effects WebGPU current row must cover layers, damage, clips, effects, texture copies, spinner work, and timestamped passes");
   }
   if (
      layerEffects.image_draws < layerEffects.image_tiles
      || layerEffects.draw_pipeline_binds <= 0
      || layerEffects.draw_bind_group_binds <= 0
      || layerEffects.draw_scissor_sets <= 0
      || layerEffects.effect_uniform_writes <= 0
      || layerEffects.render_passes <= 0
   ) {
      throw new Error(
         "layer/effects WebGPU current row must preserve visible-work and state/effect/pass counters: "
            + `items=${layerEffects.draw_items} `
            + `glyphs=${layerEffects.glyph_quads} `
            + `images=${layerEffects.image_draws} tiles=${layerEffects.image_tiles} `
            + `layers=${layerEffects.layer_draws} `
            + `damage=${layerEffects.damage_rects} `
            + `backdrops=${layerEffects.backdrop_draws} `
            + `visual_effects=${layerEffects.visual_effect_draws} `
            + `spinners=${layerEffects.spinner_draws} `
            + `pipeline_binds=${layerEffects.draw_pipeline_binds} `
            + `bind_groups=${layerEffects.draw_bind_group_binds} `
            + `scissors=${layerEffects.draw_scissor_sets} `
            + `effect_writes=${layerEffects.effect_uniform_writes} `
            + `copies=${layerEffects.texture_copies} `
            + `passes=${layerEffects.render_passes}`
      );
   }
   if (
      report.layer_effects_summary.current_p50_ms !== layerEffects.p50_ms
      || report.layer_effects_summary.current_draw_pipeline_binds !== layerEffects.draw_pipeline_binds
      || report.layer_effects_summary.current_draw_bind_group_binds !== layerEffects.draw_bind_group_binds
      || report.layer_effects_summary.current_draw_scissor_sets !== layerEffects.draw_scissor_sets
      || report.layer_effects_summary.current_effect_uniform_writes !== layerEffects.effect_uniform_writes
   ) {
      throw new Error("layer/effects WebGPU summary must match current source row");
   }
   let cleanLayer = byId.get("web.wasm.webgpu.clean_layer.clean_reuse");
   if (byId.has("web.wasm.webgpu.clean_layer.dirty_rerender")) {
      throw new Error("clean-layer dirty rerender row must stay retired from the default WebGPU report");
   }
   if (
      cleanLayer.layer_draws < cleanLayer.expected_layers
      || cleanLayer.layer_cache_hits < cleanLayer.expected_clean_hits
      || cleanLayer.layer_cache_misses !== 0
      || cleanLayer.layer_passes !== 0
      || cleanLayer.layer_cache_skipped_draws <= cleanLayer.draw_items
      || cleanLayer.gpu_timestamp_passes !== cleanLayer.render_passes
   ) {
      throw new Error(
         "clean-layer WebGPU current row must prove clean cache reuse skipped body work: "
            + `items=${cleanLayer.draw_items} `
            + `hits=${cleanLayer.layer_cache_hits} `
            + `misses=${cleanLayer.layer_cache_misses} `
            + `skipped=${cleanLayer.layer_cache_skipped_draws} `
            + `layer_passes=${cleanLayer.layer_passes} `
            + `passes=${cleanLayer.render_passes}`
      );
   }
   if (
      report.clean_layer_summary.clean_p50_ms !== cleanLayer.p50_ms
      || report.clean_layer_summary.clean_draw_items !== cleanLayer.draw_items
      || report.clean_layer_summary.clean_layer_cache_hits !== cleanLayer.layer_cache_hits
      || report.clean_layer_summary.clean_layer_cache_misses !== cleanLayer.layer_cache_misses
      || report.clean_layer_summary.clean_layer_cache_skipped_draws !== cleanLayer.layer_cache_skipped_draws
      || report.clean_layer_summary.clean_layer_passes !== cleanLayer.layer_passes
      || report.clean_layer_summary.clean_render_passes !== cleanLayer.render_passes
      || report.clean_layer_summary.clean_gpu_timestamp_total_ns !== cleanLayer.gpu_timestamp_total_ns
   ) {
      throw new Error("clean-layer WebGPU summary must match current clean source row");
   }
   let commandFamily = byId.get("web.wasm.webgpu.command_family_matrix");
   if (
      commandFamily.image_mesh_draws < commandFamily.expected_image_meshes
      || commandFamily.nine_slice_draws < commandFamily.expected_nine_slices
      || commandFamily.sdf_glyph_quads < commandFamily.expected_sdf_glyphs
      || commandFamily.expected_camera_bg !== 0
      || commandFamily.camera_bg_draws !== 0
      || commandFamily.gpu_timestamp_passes !== commandFamily.render_passes
   ) {
      throw new Error("command-family WebGPU row must cover ImageMesh, NineSlice, SDF glyph, zero web CameraBg, and timestamped passes");
   }
   if (
      report.command_family_summary.current_p50_ms !== commandFamily.p50_ms
      || report.command_family_summary.current_draw_pipeline_binds !== commandFamily.draw_pipeline_binds
      || report.command_family_summary.current_draw_bind_group_binds !== commandFamily.draw_bind_group_binds
      || report.command_family_summary.current_draw_scissor_sets !== commandFamily.draw_scissor_sets
   ) {
      throw new Error("command-family WebGPU summary must match current source row");
   }
   let glyphRun = byId.get("web.wasm.webgpu.glyph_run.current");
   if (
      glyphRun.expected_glyph_runs <= 0
      || glyphRun.expected_glyphs_per_run <= 0
      || glyphRun.expected_glyph_quads !== glyphRun.expected_glyph_runs * glyphRun.expected_glyphs_per_run
      || glyphRun.expected_sdf_glyph_quads !== glyphRun.expected_sdf_runs * glyphRun.expected_glyphs_per_run
      || glyphRun.glyph_quads !== glyphRun.expected_glyph_quads
      || glyphRun.sdf_glyph_quads !== glyphRun.expected_sdf_glyph_quads
      || glyphRun.draw_items !== glyphRun.expected_draw_items
      || glyphRun.gpu_timestamp_passes !== glyphRun.render_passes
   ) {
      throw new Error("glyph-run WebGPU current row must cover A8/SDF GlyphRun work and timestamped passes");
   }
   if (
      glyphRun.draw_pipeline_binds <= 0
      || glyphRun.draw_bind_group_binds <= 0
      || glyphRun.draw_scissor_sets <= 0
   ) {
      throw new Error(
         "glyph-run WebGPU current row must report current draw-state bind counters: "
            + `pipeline_binds=${glyphRun.draw_pipeline_binds} `
            + `bind_groups=${glyphRun.draw_bind_group_binds} `
            + `scissors=${glyphRun.draw_scissor_sets}`
      );
   }
   if (
      report.glyph_run_summary.current_p50_ms !== glyphRun.p50_ms
      || report.glyph_run_summary.current_draw_items !== glyphRun.draw_items
      || report.glyph_run_summary.current_glyph_quads !== glyphRun.glyph_quads
      || report.glyph_run_summary.current_sdf_glyph_quads !== glyphRun.sdf_glyph_quads
      || report.glyph_run_summary.current_draw_pipeline_binds !== glyphRun.draw_pipeline_binds
      || report.glyph_run_summary.current_draw_bind_group_binds !== glyphRun.draw_bind_group_binds
      || report.glyph_run_summary.current_draw_scissor_sets !== glyphRun.draw_scissor_sets
   ) {
      throw new Error("glyph-run WebGPU summary must match current source row");
   }
   let neonMarker = byId.get("web.wasm.webgpu.neon_marker.current");
   if (byId.has("web.wasm.webgpu.neon_marker.legacy_rebind")) {
      throw new Error("neon-marker legacy row must stay retired from the default WebGPU report");
   }
   if (
      neonMarker.expected_markers <= 0
      || neonMarker.expected_draw_items !== neonMarker.expected_markers * 3
      || neonMarker.draw_items !== neonMarker.expected_draw_items
      || neonMarker.solid_tris <= 0
      || neonMarker.gpu_timestamp_passes !== neonMarker.render_passes
   ) {
      throw new Error("neon marker WebGPU current row must cover marker-derived solid draws and timestamped passes");
   }
   if (
      neonMarker.draw_pipeline_binds !== 1
      || neonMarker.draw_bind_group_binds !== 0
      || neonMarker.draw_scissor_sets !== 1
   ) {
      throw new Error(
         "neon marker WebGPU current row must preserve the draw-state cached path: "
            + `pipeline_binds=${neonMarker.draw_pipeline_binds} `
            + `bind_groups=${neonMarker.draw_bind_group_binds} `
            + `scissors=${neonMarker.draw_scissor_sets}`
      );
   }
   if (
      report.neon_marker_summary.current_p50_ms !== neonMarker.p50_ms
      || report.neon_marker_summary.current_draw_items !== neonMarker.draw_items
      || report.neon_marker_summary.current_solid_tris !== neonMarker.solid_tris
      || report.neon_marker_summary.current_draw_pipeline_binds !== neonMarker.draw_pipeline_binds
      || report.neon_marker_summary.current_draw_bind_group_binds !== neonMarker.draw_bind_group_binds
      || report.neon_marker_summary.current_draw_scissor_sets !== neonMarker.draw_scissor_sets
   ) {
      throw new Error("neon marker WebGPU summary must match current source row");
   }
   let directSurface = byId.get("web.wasm.webgpu.direct_surface.current");
   if (byId.has("web.wasm.webgpu.direct_surface.legacy_scene_present")) {
      throw new Error("direct-surface legacy row must stay retired from the default WebGPU report");
   }
   if (
      directSurface.expected_draw_items <= 0
      || directSurface.expected_image_draws <= 0
      || directSurface.draw_items !== directSurface.expected_draw_items
      || directSurface.image_draws !== directSurface.expected_image_draws
      || directSurface.gpu_timestamp_passes !== directSurface.render_passes
   ) {
      throw new Error("direct-surface WebGPU current row must cover no-effect image work and timestamped passes");
   }
   if (
      directSurface.draw_passes !== 1
      || directSurface.clear_passes !== 0
      || directSurface.present_passes !== 0
      || directSurface.render_passes !== 1
      || directSurface.texture_copies !== 0
      || directSurface.gpu_timestamp_total_ns <= 0
   ) {
      throw new Error(
         "direct-surface WebGPU current row must stay on the one-pass no-scene-present route: "
            + `items=${directSurface.draw_items} `
            + `images=${directSurface.image_draws} `
            + `render_passes=${directSurface.render_passes} `
            + `draw_passes=${directSurface.draw_passes} `
            + `clear_passes=${directSurface.clear_passes} `
            + `present_passes=${directSurface.present_passes} `
            + `copies=${directSurface.texture_copies} `
            + `gpu_ns=${directSurface.gpu_timestamp_total_ns}`
      );
   }
   if (
      report.direct_surface_summary.current_p50_ms !== directSurface.p50_ms
      || report.direct_surface_summary.current_draw_items !== directSurface.draw_items
      || report.direct_surface_summary.current_render_passes !== directSurface.render_passes
      || report.direct_surface_summary.current_present_passes !== directSurface.present_passes
      || report.direct_surface_summary.current_gpu_timestamp_total_ns !== directSurface.gpu_timestamp_total_ns
      || report.direct_surface_summary.current_gpu_timestamp_passes !== directSurface.gpu_timestamp_passes
   ) {
      throw new Error("direct-surface WebGPU summary must match the current source row");
   }
   let glyphUploadCurrent = byId.get("web.wasm.webgpu.glyph_atlas_upload.current_dirty");
   let imageUploadCurrent = byId.get("web.wasm.webgpu.image_upload.current_dirty");
   if (
      byId.has("web.wasm.webgpu.glyph_atlas_upload.legacy_full")
      || byId.has("web.wasm.webgpu.image_upload.legacy_full")
   ) {
      throw new Error("default upload WebGPU report must not include retired legacy full-upload rows");
   }
   if (
      glyphUploadCurrent.glyph_quads <= 0
      || glyphUploadCurrent.gpu_timestamp_passes !== glyphUploadCurrent.render_passes
      || glyphUploadCurrent.gpu_timestamp_total_ns <= 0
      || report.upload_summary.glyph_current_gpu_timestamp_total_ns !== glyphUploadCurrent.gpu_timestamp_total_ns
      || report.upload_summary.glyph_current_texture_upload_bytes !== glyphUploadCurrent.texture_upload_bytes
      || report.upload_summary.atlas_dirty_width !== glyphUploadCurrent.dirty_width
      || report.upload_summary.atlas_dirty_height !== glyphUploadCurrent.dirty_height
   ) {
      throw new Error("glyph upload WebGPU current row must prove dirty atlas upload and timestamped passes");
   }
   if (
      imageUploadCurrent.image_draws <= 0
      || imageUploadCurrent.gpu_timestamp_passes !== imageUploadCurrent.render_passes
      || imageUploadCurrent.gpu_timestamp_total_ns <= 0
      || report.upload_summary.image_current_gpu_timestamp_total_ns !== imageUploadCurrent.gpu_timestamp_total_ns
      || report.upload_summary.image_current_texture_upload_bytes !== imageUploadCurrent.texture_upload_bytes
      || report.upload_summary.image_dirty_width !== imageUploadCurrent.dirty_width
      || report.upload_summary.image_dirty_height !== imageUploadCurrent.dirty_height
   ) {
      throw new Error("image upload WebGPU current row must prove dirty RGBA upload and timestamped passes");
   }
   let effectCurrent = byId.get("web.wasm.webgpu.effect_uniform.current_batched");
   if (byId.has("web.wasm.webgpu.effect_uniform.legacy_write_each")) {
      throw new Error("effect-uniform legacy row must stay retired from the default WebGPU report");
   }
   if (
      effectCurrent.backdrop_draws < effectCurrent.expected_backdrops
      || effectCurrent.effect_uniform_writes !== 1
      || effectCurrent.effect_uniform_bytes <= 0
      || effectCurrent.effect_uniform_slots !== effectCurrent.expected_backdrops
      || effectCurrent.gpu_timestamp_passes !== effectCurrent.render_passes
      || effectCurrent.gpu_timestamp_total_ns <= 0
      || report.effect_uniform_summary.id !== effectCurrent.id
      || report.effect_uniform_summary.current_effect_uniform_writes !== effectCurrent.effect_uniform_writes
      || report.effect_uniform_summary.current_effect_uniform_bytes !== effectCurrent.effect_uniform_bytes
      || report.effect_uniform_summary.current_effect_uniform_slots !== effectCurrent.effect_uniform_slots
      || report.effect_uniform_summary.current_backdrop_draws !== effectCurrent.backdrop_draws
      || report.effect_uniform_summary.current_texture_copies !== effectCurrent.texture_copies
      || report.effect_uniform_summary.current_render_passes !== effectCurrent.render_passes
      || report.effect_uniform_summary.current_gpu_timestamp_passes !== effectCurrent.gpu_timestamp_passes
      || report.effect_uniform_summary.current_gpu_timestamp_total_ns !== effectCurrent.gpu_timestamp_total_ns
   ) {
      throw new Error("effect-uniform WebGPU current row must prove one batched write, equivalent effect slots, and timestamped passes");
   }
   let backdropBatchCurrent = byId.get("web.wasm.webgpu.backdrop_batch.current_coalesced");
   if (
      backdropBatchCurrent.backdrop_draws < backdropBatchCurrent.expected_backdrops
      || backdropBatchCurrent.effect_uniform_slots !== backdropBatchCurrent.expected_backdrops
      || backdropBatchCurrent.effect_uniform_writes !== 1
      || backdropBatchCurrent.texture_copies !== 1
      || backdropBatchCurrent.render_passes !== 4
      || backdropBatchCurrent.gpu_timestamp_passes !== backdropBatchCurrent.render_passes
   ) {
      throw new Error("backdrop-batch WebGPU current row must cover batched effects, one texture copy, four render passes, and timestamped passes");
   }
   if (
      report.backdrop_batch_summary.current_p50_ms !== backdropBatchCurrent.p50_ms
      || report.backdrop_batch_summary.current_backdrop_draws !== backdropBatchCurrent.backdrop_draws
      || report.backdrop_batch_summary.current_texture_copies !== backdropBatchCurrent.texture_copies
      || report.backdrop_batch_summary.current_render_passes !== backdropBatchCurrent.render_passes
      || report.backdrop_batch_summary.current_gpu_timestamp_passes !== backdropBatchCurrent.gpu_timestamp_passes
   ) {
      throw new Error("backdrop-batch WebGPU summary must match current source row");
   }
}

function writeWebReports(args, url, pageReport, pixelReport, traceSummary)
{
   if (args.rawReport) {
      mkdirSync(dirname(args.rawReport), { recursive: true });
      writeFileSync(args.rawReport, `${JSON.stringify(pageReport, null, 2)}\n`);
      console.log(`wrote ${args.rawReport}`);
   }
   if (!args.jsonReport && !args.markdownReport) {
      return;
   }
   let report = buildWebReport(args, url, pageReport, pixelReport, traceSummary);
   assertWebReportContract(report);
   if (args.jsonReport) {
      mkdirSync(dirname(args.jsonReport), { recursive: true });
      writeFileSync(args.jsonReport, `${JSON.stringify(report, null, 2)}\n`);
      console.log(`wrote ${args.jsonReport}`);
   }
   if (args.markdownReport) {
      mkdirSync(dirname(args.markdownReport), { recursive: true });
      writeFileSync(args.markdownReport, renderMarkdown(report));
      console.log(`wrote ${args.markdownReport}`);
   }
}

async function captureAndCompare(args, captureUrl, out)
{
   let attempts = args.captureRetries + 1;
   let lastErr = null;
   for (let attempt = 1; attempt <= attempts; attempt++) {
      try {
         if (attempt > 1) {
            console.log(`retrying WebGPU browser capture attempt ${attempt}/${attempts}: ${lastErr.message}`);
         }
         await runChrome(args, captureUrl, out);
         let capture = loadPngRgba(out);
         assertRendered(capture, args.target);
         if (args.update) {
            mkdirSync(dirname(args.golden), { recursive: true });
            writeFileSync(args.golden, readFileSync(out));
            console.log(`updated ${args.golden}`);
         }
         let golden = loadPngRgba(args.golden);
         let diff = comparePngs(capture, golden);
         console.log(`webgpu browser golden diff: pixdiff=${diff.pixdiff} max_err=${diff.maxErr} mse=${diff.mse.toFixed(3)}`);
         if (diff.pixdiff > args.pixelTolerance || diff.maxErr > args.maxErrTolerance || diff.mse > args.mseTolerance) {
            throw new Error(
               `golden mismatch: pixdiff=${diff.pixdiff}/${args.pixelTolerance} max_err=${diff.maxErr}/${args.maxErrTolerance} mse=${diff.mse.toFixed(3)}/${args.mseTolerance}`
            );
         }
         return { capture, diff };
      } catch (err) {
         lastErr = err;
         if (attempt === attempts) {
            throw err;
         }
      }
   }
   throw lastErr || new Error("WebGPU browser capture failed without an error");
}

function waitForBrowserReport(reportPromise, timeoutMs)
{
   return Promise.race([
      reportPromise,
      new Promise((_resolvePromise, rejectPromise) => {
         setTimeout(() => rejectPromise(new Error("timed out waiting for browser perf report")), timeoutMs);
      }),
   ]);
}

function selfTestMeasurementContract()
{
   let known = distribution([5, 1, 4, 2, 3]);
   if (known.samples !== 5 || known.p50 !== 3 || known.p95 !== 5 || known.p99 !== 5 || known.max !== 5) {
      throw new Error("known percentile distribution self-test failed");
   }
   let frames = 2000;
   let stageSamples = Object.fromEntries(
      RAF_CPU_STAGE_NAMES.map(name => [name, Array.from({ length: frames }, (_, index) => index / 1000)]),
   );
   let raw = {
      frames,
      submissions: frames,
      warmup_frames: 64,
      warmup_cpu_submit_ms: Array(64).fill(0.1),
      instrumentation_overhead: {
         order: ["enabled", "disabled", "disabled", "enabled"],
         enabled_ms: Array(200).fill(0.2),
         disabled_ms: Array(200).fill(0.1),
      },
      raf_timestamps_ms: Array.from({ length: frames }, (_, index) => index * 8.333),
      raf_deltas_ms: Array(frames).fill(8.333),
      cpu_submit_ms: Array(frames).fill(0.15),
      cpu_stages_ms: stageSamples,
      cpu_stage_attribution: {},
      long_task_supported: 0,
      long_tasks: [],
      event_to_submit_status: "not-applicable-no-input-events",
      event_to_visible_status: "not-applicable-no-input-events",
      gpu_timestamp_status: "unsupported",
      gpu_timestamp_readback_skips: 0,
      queue_drain_ms: 0,
      queue_drain_raf_waits: 0,
      queue_pending_initial: 0,
      queue_pending_final: 0,
      gpu_timestamp_samples: [],
      production_path: 1,
      production_coalescing: "adjacent-order-preserving",
      production_damage_policy: "router-damage-handoff",
      submissions_per_raf: 1,
      scene_index: 0,
      resize_every_frames: 0,
      viewport_css: "1920x1080",
      device_pixel_ratio: 1,
      cross_origin_isolated: true,
   };
   let row = rafFrameCase(raw);
   if (row.samples !== frames
      || row.raf_deltas_ms.length !== frames
      || row.cpu_submit_ms.length !== frames
      || Object.values(row.cpu_stage_samples_ms).some(values => values.length !== frames)) {
      throw new Error("N displayed frames did not produce N raw frame and stage samples");
   }
   let invalid = { ...raw, raf_deltas_ms: raw.raf_deltas_ms.slice(1) };
   let rejected = false;
   try {
      rafFrameCase(invalid);
   } catch (_error) {
      rejected = true;
   }
   if (!rejected) {
      throw new Error("mismatched displayed-frame cardinality was not rejected");
   }
   console.log("measurement contract self-test passed");
}

async function main()
{
   let args = parseArgs(process.argv.slice(2));
   if (args.selfTestMeasurement) {
      selfTestMeasurementContract();
      return;
   }
   if (args.validateRawReport) {
      let pageReport = JSON.parse(readFileSync(args.validateRawReport, "utf8"));
      let traceSummary = args.traceJson
         ? await loadTraceSummary(args.traceJson, args.reportTimeoutMs)
         : null;
      if (traceSummary) {
         traceSummary.capture_phase = "benchmark-report";
         traceSummary.timing_source = "untraced-baseline-report";
      }
      let report = buildWebReport(
         args,
         persistedBrowserUrl(args),
         pageReport,
         { target: args.target, width: args.width, height: args.height, pixdiff: 0, max_err: 0, mse: 0 },
         traceSummary,
      );
      assertWebReportContract(report);
      if (args.jsonReport) {
         mkdirSync(dirname(args.jsonReport), { recursive: true });
         writeFileSync(args.jsonReport, `${JSON.stringify(report, null, 2)}\n`);
      }
      if (args.markdownReport) {
         mkdirSync(dirname(args.markdownReport), { recursive: true });
         writeFileSync(args.markdownReport, renderMarkdown(report));
      }
      console.log(`validated ${args.validateRawReport}`);
      return;
   }
   let tempDir = mkdtempSync(join(tmpdir(), "oxide-webgpu-golden-"));
   let defaultOutName =
      args.target === "id-mask"
         ? "webgpu_id_mask_compositor.png"
         : args.target === "scene3d"
           ? "webgpu_scene3d.png"
           : args.target === "glyph"
             ? "webgpu_glyph_atlas.png"
           : "webgpu_browser.png";
   let out = args.out || join(tempDir, defaultOutName);
   mkdirSync(dirname(out), { recursive: true });

   let { server, nextReportPromise } = await startServer();
   let address = server.address();
   let url = `http://127.0.0.1:${address.port}/`;
   let captureUrl = browserUrl(args, url, false);
   let browserReportUrl = browserUrl(args, url, true);
   try {
      if (args.startupReport) {
         await writeStartupReport(args, url, nextReportPromise);
         return;
      }
      if (args.canvasReport) {
         await writeCanvasDiagnosticReport(args, url, nextReportPromise);
         return;
      }
      if (args.idMaskReferenceOut) {
         let referenceUrl = new URL(browserUrl(args, url, true));
         referenceUrl.searchParams.set("id_mask_reference_only", "1");
         let pageReport = await runChromeForReport(
            { ...args, traceJson: "" },
            referenceUrl.toString(),
            nextReportPromise(),
         );
         if (typeof pageReport.id_mask_reference !== "string" || pageReport.id_mask_reference.length === 0) {
            throw new Error("browser report omitted asymmetric ID-mask reference fields");
         }
         let reference = JSON.parse(pageReport.id_mask_reference);
         mkdirSync(dirname(args.idMaskReferenceOut), { recursive: true });
         writeFileSync(args.idMaskReferenceOut, `${JSON.stringify(reference, null, 2)}\n`);
         console.log(`wrote ${args.idMaskReferenceOut}`);
         return;
      }
      if (args.idMaskMatrixOut) {
         let matrixUrl = new URL(browserUrl(args, url, true));
         matrixUrl.searchParams.set("id_mask_matrix_only", "1");
         let pageReport = await runChromeForReport(
            { ...args, traceJson: "" },
            matrixUrl.toString(),
            nextReportPromise(),
         );
         if (typeof pageReport.id_mask_matrix !== "string" || pageReport.id_mask_matrix.length === 0) {
            throw new Error("browser report omitted WebGPU ID-mask field matrix");
         }
         let matrix = JSON.parse(pageReport.id_mask_matrix);
         let expectedDimensions = [
            [256, 256],
            [512, 512],
            [1024, 1024],
            [2048, 2048],
            [257, 509],
            [2048, 257],
            [511, 1024],
         ];
         if (!Array.isArray(matrix.cases) || matrix.cases.length !== expectedDimensions.length) {
            throw new Error("WebGPU ID-mask field matrix cardinality mismatch");
         }
         for (let index = 0; index < expectedDimensions.length; index += 1) {
            let row = matrix.cases[index];
            let [width, height] = expectedDimensions[index];
            let mismatches = row.city_mismatches
               + row.neighborhood_mismatches
               + row.city_field_mismatches
               + row.seam_field_mismatches;
            if (row.width !== width
               || row.height !== height
               || row.packed_fields !== true
               || row.field_logical_bytes * 2 !== row.wide_field_logical_bytes
               || mismatches !== 0) {
               throw new Error(`WebGPU ID-mask field matrix failed at ${width}x${height}`);
            }
         }
         mkdirSync(dirname(args.idMaskMatrixOut), { recursive: true });
         writeFileSync(args.idMaskMatrixOut, `${JSON.stringify(matrix, null, 2)}\n`);
         console.log(`wrote ${args.idMaskMatrixOut}`);
         return;
      }
      if (args.reportOnly) {
         if (!args.rawReport) {
            throw new Error("--report-only requires --raw-report");
         }
         let pageReport = await runChromeForReport(
            { ...args, traceJson: "" },
            browserReportUrl,
            nextReportPromise(),
         );
         writeWebReports(
            args,
            persistedBrowserUrl(args),
            pageReport,
            { target: args.target, width: args.width, height: args.height, pixdiff: 0, maxErr: 0, mse: 0, artifact: "not-captured" },
            null,
         );
         return;
      }
      let { capture, diff } = await captureAndCompare(args, captureUrl, out);
      if (args.jsonReport || args.markdownReport || args.rawReport) {
         let reportArgs = { ...args, traceJson: "" };
         let pageReport = await runChromeForReport(reportArgs, browserReportUrl, nextReportPromise());
         let traceSummary = null;
         if (args.traceJson) {
            let traceArgs = { ...args };
            if (!traceArgs.userDataDir) {
               traceArgs.userDataDir = join(tempDir, "chrome-trace-profile");
               mkdirSync(traceArgs.userDataDir, { recursive: true });
            }
            await runChromeForReport(traceArgs, browserReportUrl, nextReportPromise());
            traceSummary = await loadTraceSummary(args.traceJson, args.reportTimeoutMs);
            traceSummary.capture_phase = "benchmark-report";
            traceSummary.timing_source = "untraced-baseline-report";
         }
         writeWebReports(args, persistedBrowserUrl(args), pageReport, {
            target: args.target,
            width: capture.width,
            height: capture.height,
            pixdiff: diff.pixdiff,
            max_err: diff.maxErr,
            mse: diff.mse,
            artifact: args.out ? out : args.golden,
         }, traceSummary);
      }
   } finally {
      let closeWait = new Promise(resolvePromise => {
         server.close(() => resolvePromise());
      });
      if (typeof server.closeIdleConnections === "function") {
         server.closeIdleConnections();
      }
      if (typeof server.closeAllConnections === "function") {
         server.closeAllConnections();
      }
      await Promise.race([
         closeWait,
         new Promise(resolvePromise => setTimeout(resolvePromise, 1000)),
      ]);
      if (!args.out) {
         rmSync(tempDir, { recursive: true, force: true });
      }
   }
}

main()
   .then(() => {
      process.exit(0);
   })
   .catch(err => {
      console.error(err.message);
      process.exit(1);
   });
