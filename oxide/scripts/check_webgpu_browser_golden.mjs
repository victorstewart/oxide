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
      virtualTimeBudget: 20000,
      captureRetries: 2,
      reportTimeoutMs: 90000,
      width: 320,
      height: 240,
      jsonReport: "",
      markdownReport: "",
      traceJson: "",
      traceCategories: "gpu,viz,cc,blink,blink.user_timing,benchmark,disabled-by-default-gpu.service,disabled-by-default-devtools.timeline",
      traceDurationMs: 5000,
      reportDate: process.env.PERF_REPORT_DATE || new Date().toISOString().slice(0, 10),
      frameSamples: 8,
      framesPerSample: 30,
      idMaskSamples: 6,
      idMaskFrames: 24,
      uploadSamples: 6,
      uploadFrames: 24,
      scene3dSamples: 6,
      scene3dFrames: 24,
      mixedSamples: 6,
      mixedFrames: 24,
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
      } else if (arg === "--json-report") {
         args.jsonReport = next();
      } else if (arg === "--markdown-report") {
         args.markdownReport = next();
      } else if (arg === "--trace-json") {
         args.traceJson = next();
      } else if (arg === "--trace-categories") {
         args.traceCategories = next();
      } else if (arg === "--trace-duration-ms") {
         args.traceDurationMs = Number(next());
      } else if (arg === "--report-date") {
         args.reportDate = next();
      } else if (arg === "--frame-samples") {
         args.frameSamples = Number(next());
      } else if (arg === "--frames-per-sample") {
         args.framesPerSample = Number(next());
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
      } else {
         throw new Error(`unknown argument ${arg}`);
      }
   }

   if (args.target !== "app" && args.target !== "id-mask" && args.target !== "scene3d") {
      throw new Error("--target must be app, id-mask, or scene3d");
   }
   if (!args.golden) {
      args.golden = defaultGoldenForTarget(args.target);
   }
   if (!Number.isFinite(args.width) || args.width <= 0 || !Number.isFinite(args.height) || args.height <= 0) {
      throw new Error("width and height must be positive numbers");
   }
   if (!Number.isFinite(args.reportTimeoutMs) || args.reportTimeoutMs <= 0) {
      throw new Error("report timeout must be a positive number");
   }
   if (!Number.isFinite(args.captureRetries) || args.captureRetries < 0) {
      throw new Error("capture retries must be a non-negative number");
   }
   if ((args.jsonReport || args.markdownReport) && !args.traceJson) {
      throw new Error("--trace-json is required when writing browser WebGPU reports");
   }
   if (args.traceJson && !args.traceCategories) {
      throw new Error("trace categories must be non-empty when --trace-json is set");
   }
   if (!Number.isFinite(args.traceDurationMs) || args.traceDurationMs <= 0) {
      throw new Error("trace duration must be a positive number");
   }
   args.reportTimeoutMs = Math.trunc(args.reportTimeoutMs);
   args.captureRetries = Math.trunc(args.captureRetries);
   args.traceDurationMs = Math.trunc(args.traceDurationMs);
   for (let key of ["frameSamples", "framesPerSample", "idMaskSamples", "idMaskFrames", "uploadSamples", "uploadFrames", "scene3dSamples", "scene3dFrames", "mixedSamples", "mixedFrames"]) {
      if (!Number.isFinite(args[key]) || args[key] <= 0) {
         throw new Error(`${key} must be a positive number`);
      }
      args[key] = Math.trunc(args[key]);
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
         res.writeHead(200, { "Content-Type": mimeType(path) });
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
         `--window-size=${args.width},${args.height}`,
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

function browserUrl(args, baseUrl, reportEndpoint)
{
   let url = new URL(baseUrl);
   url.searchParams.set("frame_samples", String(args.frameSamples));
   url.searchParams.set("frames_per_sample", String(args.framesPerSample));
   url.searchParams.set("id_mask_samples", String(args.idMaskSamples));
   url.searchParams.set("id_mask_frames", String(args.idMaskFrames));
   url.searchParams.set("upload_samples", String(args.uploadSamples));
   url.searchParams.set("upload_frames", String(args.uploadFrames));
   url.searchParams.set("scene3d_samples", String(args.scene3dSamples));
   url.searchParams.set("scene3d_frames", String(args.scene3dFrames));
   url.searchParams.set("mixed_samples", String(args.mixedSamples));
   url.searchParams.set("mixed_frames", String(args.mixedFrames));
   url.searchParams.set("capture_target", args.target);
   url.searchParams.set("capture_width", String(args.width));
   url.searchParams.set("capture_height", String(args.height));
   if (!reportEndpoint) {
      url.searchParams.set("capture_only", "1");
   }
   if (reportEndpoint) {
      url.searchParams.set("report_endpoint", "1");
   }
   return url.toString();
}

function persistedBrowserUrl(args)
{
   return `http://127.0.0.1:<ephemeral>/?frame_samples=${args.frameSamples}&frames_per_sample=${args.framesPerSample}&id_mask_samples=${args.idMaskSamples}&id_mask_frames=${args.idMaskFrames}&upload_samples=${args.uploadSamples}&upload_frames=${args.uploadFrames}&scene3d_samples=${args.scene3dSamples}&scene3d_frames=${args.scene3dFrames}&mixed_samples=${args.mixedSamples}&mixed_frames=${args.mixedFrames}&capture_target=${args.target}&capture_width=${args.width}&capture_height=${args.height}&report_endpoint=1`;
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
   } else {
      assertAppRendered(image);
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

function frameLoopCase(metrics)
{
   return {
      id: "web.wasm.webgpu.frame_loop",
      layer: "flow",
      scenario: "browser-render",
      variant: "webgpu",
      cache_state: "warm",
      refresh_mode: "browser-main-thread",
      samples: numberMetric(metrics, "samples"),
      frames_per_sample: numberMetric(metrics, "frames_per_sample"),
      frames: numberMetric(metrics, "frames"),
      p50_ms: numberMetric(metrics, "p50_ms"),
      p95_ms: numberMetric(metrics, "p95_ms"),
      p99_ms: numberMetric(metrics, "p99_ms"),
      peak_ms: numberMetric(metrics, "peak_ms"),
      avg_ms: numberMetric(metrics, "avg_ms"),
      ...pacingMetricFields(metrics, ""),
      ...allocationMetricFields(metrics, ""),
      ...frameStageAllocationMetricFields(metrics),
      draws: numberMetric(metrics, "draws"),
      draw_items: numberMetric(metrics, "draw_items"),
      draw_pipeline_binds: numberMetric(metrics, "draw_pipeline_binds"),
      draw_bind_group_binds: numberMetric(metrics, "draw_bind_group_binds"),
      draw_scissor_sets: numberMetric(metrics, "draw_scissor_sets"),
      solid_tris: numberMetric(metrics, "solid_tris"),
      image_draws: numberMetric(metrics, "image_draws"),
      image_mesh_draws: numberMetric(metrics, "image_mesh_draws"),
      nine_slice_draws: numberMetric(metrics, "nine_slice_draws"),
      glyph_quads: numberMetric(metrics, "glyph_quads"),
      sdf_glyph_quads: numberMetric(metrics, "sdf_glyph_quads"),
      clip_depth_peak: numberMetric(metrics, "clip_depth_peak"),
      damage_rects: numberMetric(metrics, "damage_rects"),
      layer_draws: numberMetric(metrics, "layer_draws"),
      scene3d_draws: numberMetric(metrics, "scene3d_draws"),
      id_mask_draws: numberMetric(metrics, "id_mask_draws"),
      backdrop_draws: numberMetric(metrics, "backdrop_draws"),
      visual_effect_draws: numberMetric(metrics, "visual_effect_draws"),
      effect_uniform_writes: numberMetric(metrics, "effect_uniform_writes"),
      effect_uniform_bytes: numberMetric(metrics, "effect_uniform_bytes"),
      effect_uniform_slots: numberMetric(metrics, "effect_uniform_slots"),
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
      unit: "ms/frame",
   };
}

function pacingMetricFields(metrics, prefix)
{
   let key = name => `${prefix}${name}`;
   return {
      frame_budget_60hz_ms: numberMetric(metrics, key("frame_budget_60hz_ms")),
      missed_frames_60hz: numberMetric(metrics, key("missed_frames_60hz")),
      missed_frame_ratio_60hz: numberMetric(metrics, key("missed_frame_ratio_60hz")),
      hitch_frames_60hz: numberMetric(metrics, key("hitch_frames_60hz")),
      hitch_ratio_60hz: numberMetric(metrics, key("hitch_ratio_60hz")),
      frame_budget_120hz_ms: numberMetric(metrics, key("frame_budget_120hz_ms")),
      missed_frames_120hz: numberMetric(metrics, key("missed_frames_120hz")),
      missed_frame_ratio_120hz: numberMetric(metrics, key("missed_frame_ratio_120hz")),
      hitch_frames_120hz: numberMetric(metrics, key("hitch_frames_120hz")),
      hitch_ratio_120hz: numberMetric(metrics, key("hitch_ratio_120hz")),
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
   };
}

function idMaskCase(metrics, id, variant, prefix)
{
   let samples = numberMetric(metrics, "samples");
   let framesPerSample = numberMetric(metrics, "frames_per_sample");
   return {
      id,
      layer: "engine",
      scenario: "browser-render",
      variant,
      cache_state: "warm",
      refresh_mode: "browser-main-thread",
      samples,
      frames_per_sample: framesPerSample,
      frames: samples * framesPerSample,
      p50_ms: numberMetric(metrics, `${prefix}_p50_ms`),
      p95_ms: numberMetric(metrics, `${prefix}_p95_ms`),
      p99_ms: numberMetric(metrics, `${prefix}_p99_ms`),
      peak_ms: numberMetric(metrics, `${prefix}_peak_ms`),
      avg_ms: numberMetric(metrics, `${prefix}_avg_ms`),
      ...pacingMetricFields(metrics, `${prefix}_`),
      ...allocationMetricFields(metrics, `${prefix}_`),
      draws: numberMetric(metrics, `${prefix}_draws`),
      draw_items: numberMetric(metrics, `${prefix}_draw_items`),
      draw_pipeline_binds: numberMetric(metrics, `${prefix}_draw_pipeline_binds`),
      draw_bind_group_binds: numberMetric(metrics, `${prefix}_draw_bind_group_binds`),
      draw_scissor_sets: numberMetric(metrics, `${prefix}_draw_scissor_sets`),
      solid_tris: numberMetric(metrics, `${prefix}_solid_tris`),
      image_draws: numberMetric(metrics, `${prefix}_image_draws`),
      image_mesh_draws: numberMetric(metrics, `${prefix}_image_mesh_draws`),
      nine_slice_draws: numberMetric(metrics, `${prefix}_nine_slice_draws`),
      glyph_quads: numberMetric(metrics, `${prefix}_glyph_quads`),
      sdf_glyph_quads: numberMetric(metrics, `${prefix}_sdf_glyph_quads`),
      clip_depth_peak: numberMetric(metrics, `${prefix}_clip_depth_peak`),
      damage_rects: numberMetric(metrics, `${prefix}_damage_rects`),
      layer_draws: numberMetric(metrics, `${prefix}_layer_draws`),
      scene3d_draws: numberMetric(metrics, `${prefix}_scene3d_draws`),
      id_mask_draws: numberMetric(metrics, `${prefix}_id_mask_draws`),
      backdrop_draws: numberMetric(metrics, `${prefix}_backdrop_draws`),
      visual_effect_draws: numberMetric(metrics, `${prefix}_visual_effect_draws`),
      effect_uniform_writes: numberMetric(metrics, `${prefix}_effect_uniform_writes`),
      effect_uniform_bytes: numberMetric(metrics, `${prefix}_effect_uniform_bytes`),
      effect_uniform_slots: numberMetric(metrics, `${prefix}_effect_uniform_slots`),
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
      unit: "ms/frame",
   };
}

function prefixedBackendCase(metrics, id, variant, prefix, extra)
{
   let samples = numberMetric(metrics, "samples");
   let framesPerSample = numberMetric(metrics, "frames_per_sample");
   return {
      id,
      layer: "engine",
      scenario: "browser-render",
      variant,
      cache_state: "warm",
      refresh_mode: "browser-main-thread",
      samples,
      frames_per_sample: framesPerSample,
      frames: samples * framesPerSample,
      p50_ms: numberMetric(metrics, `${prefix}_p50_ms`),
      p95_ms: numberMetric(metrics, `${prefix}_p95_ms`),
      p99_ms: numberMetric(metrics, `${prefix}_p99_ms`),
      peak_ms: numberMetric(metrics, `${prefix}_peak_ms`),
      avg_ms: numberMetric(metrics, `${prefix}_avg_ms`),
      ...pacingMetricFields(metrics, `${prefix}_`),
      ...allocationMetricFields(metrics, `${prefix}_`),
      draws: numberMetric(metrics, `${prefix}_draws`),
      draw_items: numberMetric(metrics, `${prefix}_draw_items`),
      draw_pipeline_binds: numberMetric(metrics, `${prefix}_draw_pipeline_binds`),
      draw_bind_group_binds: numberMetric(metrics, `${prefix}_draw_bind_group_binds`),
      draw_scissor_sets: numberMetric(metrics, `${prefix}_draw_scissor_sets`),
      solid_tris: numberMetric(metrics, `${prefix}_solid_tris`),
      image_draws: numberMetric(metrics, `${prefix}_image_draws`),
      image_mesh_draws: numberMetric(metrics, `${prefix}_image_mesh_draws`),
      nine_slice_draws: numberMetric(metrics, `${prefix}_nine_slice_draws`),
      glyph_quads: numberMetric(metrics, `${prefix}_glyph_quads`),
      sdf_glyph_quads: numberMetric(metrics, `${prefix}_sdf_glyph_quads`),
      clip_depth_peak: numberMetric(metrics, `${prefix}_clip_depth_peak`),
      damage_rects: numberMetric(metrics, `${prefix}_damage_rects`),
      layer_draws: numberMetric(metrics, `${prefix}_layer_draws`),
      scene3d_draws: numberMetric(metrics, `${prefix}_scene3d_draws`),
      id_mask_draws: numberMetric(metrics, `${prefix}_id_mask_draws`),
      backdrop_draws: numberMetric(metrics, `${prefix}_backdrop_draws`),
      visual_effect_draws: numberMetric(metrics, `${prefix}_visual_effect_draws`),
      effect_uniform_writes: numberMetric(metrics, `${prefix}_effect_uniform_writes`),
      effect_uniform_bytes: numberMetric(metrics, `${prefix}_effect_uniform_bytes`),
      effect_uniform_slots: numberMetric(metrics, `${prefix}_effect_uniform_slots`),
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
      unit: "ms/frame",
   };
}

const WARM_RESOURCE_CHURN_EXCLUDED_IDS = new Set([
   "web.wasm.webgpu.id_mask_compositor.legacy_upload",
   "web.wasm.webgpu.glyph_atlas_upload.legacy_full",
   "web.wasm.webgpu.image_upload.legacy_full",
   "web.wasm.webgpu.upload_scratch.legacy_temp_alloc",
   "web.wasm.webgpu.effect_uniform.legacy_write_each",
   "web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy",
   "web.wasm.webgpu.scene3d.recreate_mesh",
   "web.wasm.webgpu.scene3d.stress_recreate_mesh",
   "web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched",
   "web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched",
   "web.wasm.webgpu.command_family_matrix.legacy_rebind",
   "web.wasm.webgpu.draw_state_cache.legacy_rebind",
   "web.wasm.webgpu.clip_state_cache.legacy_rebind",
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

const EXPECTED_BENCHMARK_MARKS = [
   "frame_loop",
   "id_mask_ab",
   "upload_ab",
   "upload_scratch_ab",
   "effect_uniform_ab",
   "backdrop_batch_ab",
   "scene3d_ab",
   "mixed_matrix",
   "layer_effects_matrix",
   "command_family_matrix",
   "draw_state_cache_ab",
   "clip_state_cache_ab",
];

const WEBGPU_BACKEND_PATHS = [
   {
      id: "frame_loop",
      rows: ["web.wasm.webgpu.frame_loop"],
      counters: ["draws", "draw_items", "draw_passes", "command_buffers", "buffer_upload_bytes", "gpu_timestamp_passes"],
      comparison: "coverage",
   },
   {
      id: "id_mask_compositor",
      rows: ["web.wasm.webgpu.id_mask_compositor.current", "web.wasm.webgpu.id_mask_compositor.legacy_upload"],
      counters: ["id_mask_draws", "id_mask_raster_passes", "id_mask_field_seed_passes", "id_mask_field_jump_passes", "id_mask_compositor_passes", "buffer_upload_bytes", "vertices", "gpu_timestamp_passes"],
      comparison: "current_vs_legacy",
   },
   {
      id: "glyph_atlas_upload",
      rows: ["web.wasm.webgpu.glyph_atlas_upload.current_dirty", "web.wasm.webgpu.glyph_atlas_upload.legacy_full"],
      counters: ["glyph_quads", "texture_upload_bytes", "buffer_upload_bytes", "gpu_timestamp_passes"],
      comparison: "current_vs_legacy",
   },
   {
      id: "image_upload",
      rows: ["web.wasm.webgpu.image_upload.current_dirty", "web.wasm.webgpu.image_upload.legacy_full"],
      counters: ["image_draws", "texture_upload_bytes", "buffer_upload_bytes", "gpu_timestamp_passes"],
      comparison: "current_vs_legacy",
   },
   {
      id: "upload_scratch",
      rows: ["web.wasm.webgpu.upload_scratch.current_reuse", "web.wasm.webgpu.upload_scratch.legacy_temp_alloc"],
      counters: ["image_upload_temp_allocs", "image_upload_temp_bytes", "image_upload_scratch_bytes", "texture_upload_bytes", "gpu_timestamp_passes"],
      comparison: "current_vs_legacy",
   },
   {
      id: "effect_uniform",
      rows: ["web.wasm.webgpu.effect_uniform.current_batched", "web.wasm.webgpu.effect_uniform.legacy_write_each"],
      counters: ["backdrop_draws", "visual_effect_draws", "effect_uniform_writes", "effect_uniform_slots", "texture_copies", "render_passes", "gpu_timestamp_total_ns"],
      comparison: "current_vs_legacy",
   },
   {
      id: "backdrop_batch",
      rows: ["web.wasm.webgpu.backdrop_batch.current_coalesced", "web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy"],
      counters: ["backdrop_draws", "effect_uniform_slots", "texture_copies", "render_passes", "gpu_timestamp_passes"],
      comparison: "current_vs_legacy",
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
      rows: ["web.wasm.webgpu.mixed_text_image_effects", "web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched"],
      counters: ["glyph_quads", "image_draws", "image_tiles", "backdrop_draws", "visual_effect_draws", "spinner_draws", "layer_draws", "damage_rects", "draw_pipeline_binds", "draw_bind_group_binds", "draw_scissor_sets", "effect_uniform_writes", "texture_copies", "render_passes", "gpu_timestamp_passes"],
      comparison: "current_vs_legacy",
   },
   {
      id: "layer_damage_effects",
      rows: ["web.wasm.webgpu.layer_damage_effects", "web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched"],
      counters: ["glyph_quads", "image_draws", "image_tiles", "layer_draws", "damage_rects", "clip_depth_peak", "backdrop_draws", "visual_effect_draws", "spinner_draws", "draw_pipeline_binds", "draw_bind_group_binds", "draw_scissor_sets", "effect_uniform_writes", "texture_copies", "render_passes", "gpu_timestamp_passes"],
      comparison: "current_vs_legacy",
   },
   {
      id: "command_family_matrix",
      rows: ["web.wasm.webgpu.command_family_matrix", "web.wasm.webgpu.command_family_matrix.legacy_rebind"],
      counters: ["image_mesh_draws", "nine_slice_draws", "sdf_glyph_quads", "camera_bg_draws", "expected_camera_bg", "draw_items", "draw_pipeline_binds", "draw_bind_group_binds", "draw_scissor_sets", "gpu_timestamp_passes"],
      comparison: "current_vs_legacy",
   },
   {
      id: "draw_state_cache",
      rows: ["web.wasm.webgpu.draw_state_cache.current", "web.wasm.webgpu.draw_state_cache.legacy_rebind"],
      counters: ["draw_items", "draw_pipeline_binds", "draw_bind_group_binds", "draw_scissor_sets", "gpu_timestamp_passes"],
      comparison: "current_vs_legacy",
   },
   {
      id: "clip_state_cache",
      rows: ["web.wasm.webgpu.clip_state_cache.current", "web.wasm.webgpu.clip_state_cache.legacy_rebind"],
      counters: ["clip_depth_peak", "draw_scissor_sets", "draw_pipeline_binds", "draw_bind_group_binds", "gpu_timestamp_passes"],
      comparison: "current_vs_legacy",
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
   let reference = allocationSummary.row_details.find(row => row.id === "web.wasm.webgpu.frame_loop");
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
   let frame = cases.find(row => row.id === "web.wasm.webgpu.frame_loop");
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
         rowDetails.push({
            id: rowId,
            p50_ms: row.p50_ms,
            p95_ms: row.p95_ms,
            p99_ms: row.p99_ms,
            peak_ms: row.peak_ms,
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
   let idMaskMetrics = parseMetricString(pageReport.id_mask_ab);
   let uploadMetrics = parseMetricString(pageReport.upload_ab);
   let uploadScratchMetrics = parseMetricString(pageReport.upload_scratch_ab);
   let effectUniformMetrics = parseMetricString(pageReport.effect_uniform_ab);
   let backdropBatchMetrics = parseMetricString(pageReport.backdrop_batch_ab);
   let scene3dMetrics = parseMetricString(pageReport.scene3d_ab);
   let mixedMetrics = parseMetricString(pageReport.mixed_matrix);
   let layerEffectsMetrics = parseMetricString(pageReport.layer_effects_matrix);
   let commandFamilyMetrics = parseMetricString(pageReport.command_family_matrix);
   let drawStateMetrics = parseMetricString(pageReport.draw_state_cache_ab);
   let clipStateMetrics = parseMetricString(pageReport.clip_state_ab);
   let timingMetrics = parseMetricString(pageReport.webgpu_timing);
   let cases = [
      frameLoopCase(frameMetrics),
      idMaskCase(
         idMaskMetrics,
         "web.wasm.webgpu.id_mask_compositor.current",
         "webgpu-current",
         "current",
      ),
      idMaskCase(
         idMaskMetrics,
         "web.wasm.webgpu.id_mask_compositor.legacy_upload",
         "webgpu-legacy-upload",
         "legacy",
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
         "web.wasm.webgpu.glyph_atlas_upload.legacy_full",
         "webgpu-full-atlas-update",
         "glyph_legacy",
         {
            atlas_width: numberMetric(uploadMetrics, "atlas_width"),
            atlas_height: numberMetric(uploadMetrics, "atlas_height"),
            dirty_width: numberMetric(uploadMetrics, "atlas_width"),
            dirty_height: numberMetric(uploadMetrics, "atlas_height"),
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
         uploadMetrics,
         "web.wasm.webgpu.image_upload.legacy_full",
         "webgpu-full-rgba-update",
         "image_legacy",
         {
            image_width: numberMetric(uploadMetrics, "image_width"),
            image_height: numberMetric(uploadMetrics, "image_height"),
            dirty_width: numberMetric(uploadMetrics, "image_width"),
            dirty_height: numberMetric(uploadMetrics, "image_height"),
         },
      ),
      prefixedBackendCase(
         uploadScratchMetrics,
         "web.wasm.webgpu.upload_scratch.current_reuse",
         "webgpu-upload-scratch-current-reuse",
         "current",
         {
            updates: numberMetric(uploadScratchMetrics, "updates"),
            atlas_dirty_width: numberMetric(uploadScratchMetrics, "atlas_dirty_width"),
            atlas_dirty_height: numberMetric(uploadScratchMetrics, "atlas_dirty_height"),
            image_dirty_width: numberMetric(uploadScratchMetrics, "image_dirty_width"),
            image_dirty_height: numberMetric(uploadScratchMetrics, "image_dirty_height"),
         },
      ),
      prefixedBackendCase(
         uploadScratchMetrics,
         "web.wasm.webgpu.upload_scratch.legacy_temp_alloc",
         "webgpu-upload-scratch-legacy-temp-alloc",
         "legacy",
         {
            updates: numberMetric(uploadScratchMetrics, "updates"),
            atlas_dirty_width: numberMetric(uploadScratchMetrics, "atlas_dirty_width"),
            atlas_dirty_height: numberMetric(uploadScratchMetrics, "atlas_dirty_height"),
            image_dirty_width: numberMetric(uploadScratchMetrics, "image_dirty_width"),
            image_dirty_height: numberMetric(uploadScratchMetrics, "image_dirty_height"),
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
         effectUniformMetrics,
         "web.wasm.webgpu.effect_uniform.legacy_write_each",
         "webgpu-effect-uniform-legacy-write-each",
         "legacy",
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
         backdropBatchMetrics,
         "web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy",
         "webgpu-backdrop-batch-legacy-per-backdrop-copy",
         "legacy",
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
         mixedMetrics,
         "web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched",
         "webgpu-mixed-effects-legacy-rebind-unbatched",
         "legacy",
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
         layerEffectsMetrics,
         "web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched",
         "webgpu-layer-damage-effects-legacy-rebind-unbatched",
         "legacy",
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
         commandFamilyMetrics,
         "web.wasm.webgpu.command_family_matrix.legacy_rebind",
         "webgpu-command-family-legacy-rebind",
         "legacy",
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
         drawStateMetrics,
         "web.wasm.webgpu.draw_state_cache.current",
         "webgpu-draw-state-cache-current",
         "current",
         {
            expected_draw_items: numberMetric(drawStateMetrics, "expected_draw_items"),
            columns: numberMetric(drawStateMetrics, "columns"),
            image_width: numberMetric(drawStateMetrics, "image_width"),
            image_height: numberMetric(drawStateMetrics, "image_height"),
         },
      ),
      prefixedBackendCase(
         drawStateMetrics,
         "web.wasm.webgpu.draw_state_cache.legacy_rebind",
         "webgpu-draw-state-cache-legacy-rebind",
         "legacy",
         {
            expected_draw_items: numberMetric(drawStateMetrics, "expected_draw_items"),
            columns: numberMetric(drawStateMetrics, "columns"),
            image_width: numberMetric(drawStateMetrics, "image_width"),
            image_height: numberMetric(drawStateMetrics, "image_height"),
         },
      ),
      prefixedBackendCase(
         clipStateMetrics,
         "web.wasm.webgpu.clip_state_cache.current",
         "webgpu-clip-state-cache-current",
         "current",
         {
            expected_draw_items: numberMetric(clipStateMetrics, "expected_draw_items"),
            expected_clip_runs: numberMetric(clipStateMetrics, "expected_clip_runs"),
            expected_clip_depth: numberMetric(clipStateMetrics, "expected_clip_depth"),
            image_width: numberMetric(clipStateMetrics, "image_width"),
            image_height: numberMetric(clipStateMetrics, "image_height"),
         },
      ),
      prefixedBackendCase(
         clipStateMetrics,
         "web.wasm.webgpu.clip_state_cache.legacy_rebind",
         "webgpu-clip-state-cache-legacy-rebind",
         "legacy",
         {
            expected_draw_items: numberMetric(clipStateMetrics, "expected_draw_items"),
            expected_clip_runs: numberMetric(clipStateMetrics, "expected_clip_runs"),
            expected_clip_depth: numberMetric(clipStateMetrics, "expected_clip_depth"),
            image_width: numberMetric(clipStateMetrics, "image_width"),
            image_height: numberMetric(clipStateMetrics, "image_height"),
         },
      ),
   ];
   let timestampRows = cases.filter(row => row.gpu_timestamp_passes > 0);
   let timestampQuery = stringMetric(timingMetrics, "timestamp_query");
   let timestampCollected = timestampRows.length > 0;
   let timestampPasses = timestampRows.reduce((sum, row) => sum + row.gpu_timestamp_passes, 0);
   let timestampTotalNs = timestampRows.reduce((sum, row) => sum + row.gpu_timestamp_total_ns, 0);
   let warmResourceChurn = warmResourceChurnSummary(cases);
   let wasmAllocationAudit = wasmAllocationSummary(cases);
   let wasmAllocationInvariance = wasmAllocationInvarianceSummary(wasmAllocationAudit);
   let backendPathCoverage = backendPathCoverageSummary(cases);
   let benchmarkMarks = benchmarkMarkSummary(pageReport, traceSummary);

   return {
      version: 2,
      suite: "web-wasm",
      generated_date: args.reportDate,
      browser_target: args.chromeArch
         ? `Chrome ${args.chromeArch} via headless CLI`
         : "Chrome via headless CLI",
      capture_target: args.target,
      url,
      status: "browser-baseline",
      notes: [
         "BrowserRenderer selected the WebGPU backend through async renderer initialization.",
         "This baseline was collected from a release wasm build served through the static web host.",
         "Production web visual startup is WebGPU-only; unsupported browsers return NOT SUPPORTED instead of drawing through Canvas2D.",
         "The WebGPU ID-mask current and legacy upload rows are captured in the same browser process and scene contract.",
         "The WebGPU effect-uniform A/B rows draw the same backdrop scene while comparing one batched dynamic-uniform upload against one queue write per backdrop.",
         "The WebGPU backdrop-batch A/B rows draw separated consecutive backdrops while comparing one shared scene-copy pass against the legacy per-backdrop copy path.",
         "The WebGPU layer/damage/effects A/B rows draw the same nested layer, damage, image, glyph, backdrop, visual-effect, and spinner workload while comparing current state/effect batching against legacy rebinding/unbatched toggles.",
         "The WebGPU command-family A/B rows draw the same generic ImageMesh, NineSlice, and SDF glyph workload while comparing current draw-state caching against a legacy rebind path and keeping web CameraBg work unavailable.",
         "The WebGPU clip-state A/B rows use real Oxide ClipPush/ClipPop commands to measure scissor-state caching.",
         "Pass-family counters provide browser GPU-stage attribution when direct timestamp queries are unavailable.",
         "Warm current-path WebGPU rows are gated against post-warmup resource creation, buffer growth, mesh creation, image-upload temp allocation, and CPU/image scratch growth.",
         "WASM allocation counters measure Rust allocator activity inside each post-warmup benchmark frame loop and are reported separately from renderer-owned resource churn.",
         "Chrome startup tracing is captured from a duplicate benchmark-report run so GPU/browser-process activity is tied to the same report workload without perturbing persisted timing rows.",
         "Browser User Timing marks surround every benchmark family and are persisted to prove the traced report run exercised the expected workload phases.",
      ],
      smoke: {
         platform: pageReport.platform,
         webgpu: pageReport.webgpu,
         webgpu_timing: pageReport.webgpu_timing,
         backend: pageReport.backend,
         render: pageReport.render,
         upload_ab: pageReport.upload_ab,
         upload_scratch_ab: pageReport.upload_scratch_ab,
         effect_uniform_ab: pageReport.effect_uniform_ab,
         backdrop_batch_ab: pageReport.backdrop_batch_ab,
         scene3d_ab: pageReport.scene3d_ab,
         mixed_matrix: pageReport.mixed_matrix,
         layer_effects_matrix: pageReport.layer_effects_matrix,
         command_family_matrix: pageReport.command_family_matrix,
         draw_state_cache_ab: pageReport.draw_state_cache_ab,
         clip_state_ab: pageReport.clip_state_ab,
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
      browser_trace: traceSummary,
      benchmark_marks: benchmarkMarks,
      warm_resource_churn: warmResourceChurn,
      wasm_allocation_audit: wasmAllocationAudit,
      wasm_allocation_invariance: wasmAllocationInvariance,
      frame_loop_wasm_allocation_stages: frameLoopWasmStageSummary(cases),
      backend_path_coverage: backendPathCoverage,
      cases,
      ab_summary: {
         id: "web.wasm.webgpu.id_mask_compositor.current_vs_legacy_upload",
         legacy_over_current: numberMetric(idMaskMetrics, "legacy_over_current"),
         current_p50_ms: numberMetric(idMaskMetrics, "current_p50_ms"),
         legacy_p50_ms: numberMetric(idMaskMetrics, "legacy_p50_ms"),
         current_render_passes: numberMetric(idMaskMetrics, "current_render_passes"),
         legacy_render_passes: numberMetric(idMaskMetrics, "legacy_render_passes"),
         current_buffer_upload_bytes: numberMetric(idMaskMetrics, "current_buffer_upload_bytes"),
         legacy_buffer_upload_bytes: numberMetric(idMaskMetrics, "legacy_buffer_upload_bytes"),
         vertices: numberMetric(idMaskMetrics, "vertices"),
         vertex_bytes: numberMetric(idMaskMetrics, "vertex_bytes"),
      },
      upload_summary: {
         id: "web.wasm.webgpu.upload.current_dirty_vs_legacy_full",
         glyph_legacy_over_current: numberMetric(uploadMetrics, "glyph_legacy_over_current"),
         image_legacy_over_current: numberMetric(uploadMetrics, "image_legacy_over_current"),
         glyph_current_p50_ms: numberMetric(uploadMetrics, "glyph_current_p50_ms"),
         glyph_legacy_p50_ms: numberMetric(uploadMetrics, "glyph_legacy_p50_ms"),
         image_current_p50_ms: numberMetric(uploadMetrics, "image_current_p50_ms"),
         image_legacy_p50_ms: numberMetric(uploadMetrics, "image_legacy_p50_ms"),
         glyph_current_texture_upload_bytes: numberMetric(uploadMetrics, "glyph_current_texture_upload_bytes"),
         glyph_legacy_texture_upload_bytes: numberMetric(uploadMetrics, "glyph_legacy_texture_upload_bytes"),
         image_current_texture_upload_bytes: numberMetric(uploadMetrics, "image_current_texture_upload_bytes"),
         image_legacy_texture_upload_bytes: numberMetric(uploadMetrics, "image_legacy_texture_upload_bytes"),
         glyph_current_gpu_timestamp_total_ns: numberMetric(uploadMetrics, "glyph_current_gpu_timestamp_total_ns"),
         glyph_legacy_gpu_timestamp_total_ns: numberMetric(uploadMetrics, "glyph_legacy_gpu_timestamp_total_ns"),
         glyph_legacy_gpu_over_current:
            numberMetric(uploadMetrics, "glyph_legacy_gpu_timestamp_total_ns")
            / numberMetric(uploadMetrics, "glyph_current_gpu_timestamp_total_ns"),
         image_current_gpu_timestamp_total_ns: numberMetric(uploadMetrics, "image_current_gpu_timestamp_total_ns"),
         image_legacy_gpu_timestamp_total_ns: numberMetric(uploadMetrics, "image_legacy_gpu_timestamp_total_ns"),
         image_legacy_gpu_over_current:
            numberMetric(uploadMetrics, "image_legacy_gpu_timestamp_total_ns")
            / numberMetric(uploadMetrics, "image_current_gpu_timestamp_total_ns"),
      },
      upload_scratch_summary: {
         id: "web.wasm.webgpu.upload_scratch.current_reuse_vs_legacy_temp_alloc",
         legacy_over_current: numberMetric(uploadScratchMetrics, "legacy_over_current"),
         current_p50_ms: numberMetric(uploadScratchMetrics, "current_p50_ms"),
         legacy_p50_ms: numberMetric(uploadScratchMetrics, "legacy_p50_ms"),
         current_temp_allocs: numberMetric(uploadScratchMetrics, "current_image_upload_temp_allocs"),
         legacy_temp_allocs: numberMetric(uploadScratchMetrics, "legacy_image_upload_temp_allocs"),
         current_temp_bytes: numberMetric(uploadScratchMetrics, "current_image_upload_temp_bytes"),
         legacy_temp_bytes: numberMetric(uploadScratchMetrics, "legacy_image_upload_temp_bytes"),
         current_scratch_bytes: numberMetric(uploadScratchMetrics, "current_image_upload_scratch_bytes"),
         legacy_scratch_bytes: numberMetric(uploadScratchMetrics, "legacy_image_upload_scratch_bytes"),
         current_scratch_grows: numberMetric(uploadScratchMetrics, "current_image_upload_scratch_grows"),
         legacy_scratch_grows: numberMetric(uploadScratchMetrics, "legacy_image_upload_scratch_grows"),
         current_texture_upload_bytes: numberMetric(uploadScratchMetrics, "current_texture_upload_bytes"),
         legacy_texture_upload_bytes: numberMetric(uploadScratchMetrics, "legacy_texture_upload_bytes"),
         updates: numberMetric(uploadScratchMetrics, "updates"),
      },
      effect_uniform_summary: {
         id: "web.wasm.webgpu.effect_uniform.batched_vs_legacy_write_each",
         legacy_over_current: numberMetric(effectUniformMetrics, "legacy_over_current"),
         current_p50_ms: numberMetric(effectUniformMetrics, "current_p50_ms"),
         legacy_p50_ms: numberMetric(effectUniformMetrics, "legacy_p50_ms"),
         current_effect_uniform_writes: numberMetric(
            effectUniformMetrics,
            "current_effect_uniform_writes",
         ),
         legacy_effect_uniform_writes: numberMetric(
            effectUniformMetrics,
            "legacy_effect_uniform_writes",
         ),
         current_effect_uniform_bytes: numberMetric(
            effectUniformMetrics,
            "current_effect_uniform_bytes",
         ),
         legacy_effect_uniform_bytes: numberMetric(
            effectUniformMetrics,
            "legacy_effect_uniform_bytes",
         ),
         current_effect_uniform_slots: numberMetric(
            effectUniformMetrics,
            "current_effect_uniform_slots",
         ),
         legacy_effect_uniform_slots: numberMetric(
            effectUniformMetrics,
            "legacy_effect_uniform_slots",
         ),
         current_backdrop_draws: numberMetric(effectUniformMetrics, "current_backdrop_draws"),
         legacy_backdrop_draws: numberMetric(effectUniformMetrics, "legacy_backdrop_draws"),
         current_texture_copies: numberMetric(effectUniformMetrics, "current_texture_copies"),
         legacy_texture_copies: numberMetric(effectUniformMetrics, "legacy_texture_copies"),
         current_render_passes: numberMetric(effectUniformMetrics, "current_render_passes"),
         legacy_render_passes: numberMetric(effectUniformMetrics, "legacy_render_passes"),
         current_gpu_timestamp_total_ns: numberMetric(
            effectUniformMetrics,
            "current_gpu_timestamp_total_ns",
         ),
         legacy_gpu_timestamp_total_ns: numberMetric(
            effectUniformMetrics,
            "legacy_gpu_timestamp_total_ns",
         ),
         legacy_gpu_over_current:
            numberMetric(effectUniformMetrics, "legacy_gpu_timestamp_total_ns")
            / numberMetric(effectUniformMetrics, "current_gpu_timestamp_total_ns"),
         expected_backdrops: numberMetric(effectUniformMetrics, "expected_backdrops"),
      },
      backdrop_batch_summary: {
         id: "web.wasm.webgpu.backdrop_batch.coalesced_vs_per_backdrop_copy",
         legacy_over_current: numberMetric(backdropBatchMetrics, "legacy_over_current"),
         current_p50_ms: numberMetric(backdropBatchMetrics, "current_p50_ms"),
         legacy_p50_ms: numberMetric(backdropBatchMetrics, "legacy_p50_ms"),
         current_effect_uniform_writes: numberMetric(
            backdropBatchMetrics,
            "current_effect_uniform_writes",
         ),
         legacy_effect_uniform_writes: numberMetric(
            backdropBatchMetrics,
            "legacy_effect_uniform_writes",
         ),
         current_effect_uniform_slots: numberMetric(
            backdropBatchMetrics,
            "current_effect_uniform_slots",
         ),
         legacy_effect_uniform_slots: numberMetric(
            backdropBatchMetrics,
            "legacy_effect_uniform_slots",
         ),
         current_backdrop_draws: numberMetric(backdropBatchMetrics, "current_backdrop_draws"),
         legacy_backdrop_draws: numberMetric(backdropBatchMetrics, "legacy_backdrop_draws"),
         current_texture_copies: numberMetric(backdropBatchMetrics, "current_texture_copies"),
         legacy_texture_copies: numberMetric(backdropBatchMetrics, "legacy_texture_copies"),
         current_render_passes: numberMetric(backdropBatchMetrics, "current_render_passes"),
         legacy_render_passes: numberMetric(backdropBatchMetrics, "legacy_render_passes"),
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
         id: "web.wasm.webgpu.mixed_text_image_effects.current_vs_legacy_rebind_unbatched",
         legacy_over_current: numberMetric(mixedMetrics, "legacy_over_current"),
         current_p50_ms: numberMetric(mixedMetrics, "current_p50_ms"),
         legacy_p50_ms: numberMetric(mixedMetrics, "legacy_p50_ms"),
         current_draw_items: numberMetric(mixedMetrics, "current_draw_items"),
         legacy_draw_items: numberMetric(mixedMetrics, "legacy_draw_items"),
         current_draw_pipeline_binds: numberMetric(mixedMetrics, "current_draw_pipeline_binds"),
         legacy_draw_pipeline_binds: numberMetric(mixedMetrics, "legacy_draw_pipeline_binds"),
         current_draw_bind_group_binds: numberMetric(mixedMetrics, "current_draw_bind_group_binds"),
         legacy_draw_bind_group_binds: numberMetric(mixedMetrics, "legacy_draw_bind_group_binds"),
         current_draw_scissor_sets: numberMetric(mixedMetrics, "current_draw_scissor_sets"),
         legacy_draw_scissor_sets: numberMetric(mixedMetrics, "legacy_draw_scissor_sets"),
         current_effect_uniform_writes: numberMetric(mixedMetrics, "current_effect_uniform_writes"),
         legacy_effect_uniform_writes: numberMetric(mixedMetrics, "legacy_effect_uniform_writes"),
         current_texture_copies: numberMetric(mixedMetrics, "current_texture_copies"),
         legacy_texture_copies: numberMetric(mixedMetrics, "legacy_texture_copies"),
         current_render_passes: numberMetric(mixedMetrics, "current_render_passes"),
         legacy_render_passes: numberMetric(mixedMetrics, "legacy_render_passes"),
         current_glyph_quads: numberMetric(mixedMetrics, "current_glyph_quads"),
         legacy_glyph_quads: numberMetric(mixedMetrics, "legacy_glyph_quads"),
         current_image_draws: numberMetric(mixedMetrics, "current_image_draws"),
         legacy_image_draws: numberMetric(mixedMetrics, "legacy_image_draws"),
         image_tiles: numberMetric(mixedMetrics, "image_tiles"),
         current_backdrop_draws: numberMetric(mixedMetrics, "current_backdrop_draws"),
         legacy_backdrop_draws: numberMetric(mixedMetrics, "legacy_backdrop_draws"),
         current_visual_effect_draws: numberMetric(mixedMetrics, "current_visual_effect_draws"),
         legacy_visual_effect_draws: numberMetric(mixedMetrics, "legacy_visual_effect_draws"),
         current_layer_draws: numberMetric(mixedMetrics, "current_layer_draws"),
         legacy_layer_draws: numberMetric(mixedMetrics, "legacy_layer_draws"),
         current_damage_rects: numberMetric(mixedMetrics, "current_damage_rects"),
         legacy_damage_rects: numberMetric(mixedMetrics, "legacy_damage_rects"),
      },
      layer_effects_summary: {
         id: "web.wasm.webgpu.layer_damage_effects.current_vs_legacy_rebind_unbatched",
         legacy_over_current: numberMetric(layerEffectsMetrics, "legacy_over_current"),
         current_p50_ms: numberMetric(layerEffectsMetrics, "current_p50_ms"),
         legacy_p50_ms: numberMetric(layerEffectsMetrics, "legacy_p50_ms"),
         current_draw_items: numberMetric(layerEffectsMetrics, "current_draw_items"),
         legacy_draw_items: numberMetric(layerEffectsMetrics, "legacy_draw_items"),
         current_draw_pipeline_binds: numberMetric(layerEffectsMetrics, "current_draw_pipeline_binds"),
         legacy_draw_pipeline_binds: numberMetric(layerEffectsMetrics, "legacy_draw_pipeline_binds"),
         current_draw_bind_group_binds: numberMetric(layerEffectsMetrics, "current_draw_bind_group_binds"),
         legacy_draw_bind_group_binds: numberMetric(layerEffectsMetrics, "legacy_draw_bind_group_binds"),
         current_draw_scissor_sets: numberMetric(layerEffectsMetrics, "current_draw_scissor_sets"),
         legacy_draw_scissor_sets: numberMetric(layerEffectsMetrics, "legacy_draw_scissor_sets"),
         current_effect_uniform_writes: numberMetric(layerEffectsMetrics, "current_effect_uniform_writes"),
         legacy_effect_uniform_writes: numberMetric(layerEffectsMetrics, "legacy_effect_uniform_writes"),
         current_texture_copies: numberMetric(layerEffectsMetrics, "current_texture_copies"),
         legacy_texture_copies: numberMetric(layerEffectsMetrics, "legacy_texture_copies"),
         current_render_passes: numberMetric(layerEffectsMetrics, "current_render_passes"),
         legacy_render_passes: numberMetric(layerEffectsMetrics, "legacy_render_passes"),
         current_glyph_quads: numberMetric(layerEffectsMetrics, "current_glyph_quads"),
         legacy_glyph_quads: numberMetric(layerEffectsMetrics, "legacy_glyph_quads"),
         current_image_draws: numberMetric(layerEffectsMetrics, "current_image_draws"),
         legacy_image_draws: numberMetric(layerEffectsMetrics, "legacy_image_draws"),
         image_tiles: numberMetric(layerEffectsMetrics, "image_tiles"),
         current_backdrop_draws: numberMetric(layerEffectsMetrics, "current_backdrop_draws"),
         legacy_backdrop_draws: numberMetric(layerEffectsMetrics, "legacy_backdrop_draws"),
         current_visual_effect_draws: numberMetric(layerEffectsMetrics, "current_visual_effect_draws"),
         legacy_visual_effect_draws: numberMetric(layerEffectsMetrics, "legacy_visual_effect_draws"),
         current_spinner_draws: numberMetric(layerEffectsMetrics, "current_spinner_draws"),
         legacy_spinner_draws: numberMetric(layerEffectsMetrics, "legacy_spinner_draws"),
         current_layer_draws: numberMetric(layerEffectsMetrics, "current_layer_draws"),
         legacy_layer_draws: numberMetric(layerEffectsMetrics, "legacy_layer_draws"),
         current_damage_rects: numberMetric(layerEffectsMetrics, "current_damage_rects"),
         legacy_damage_rects: numberMetric(layerEffectsMetrics, "legacy_damage_rects"),
         expected_layers: numberMetric(layerEffectsMetrics, "expected_layers"),
         expected_damage_rects: numberMetric(layerEffectsMetrics, "expected_damage_rects"),
         expected_backdrops: numberMetric(layerEffectsMetrics, "expected_backdrops"),
      },
      command_family_summary: {
         id: "web.wasm.webgpu.command_family_matrix.current_vs_legacy_rebind",
         legacy_over_current: numberMetric(commandFamilyMetrics, "legacy_over_current"),
         current_p50_ms: numberMetric(commandFamilyMetrics, "current_p50_ms"),
         legacy_p50_ms: numberMetric(commandFamilyMetrics, "legacy_p50_ms"),
         current_draw_items: numberMetric(commandFamilyMetrics, "current_draw_items"),
         legacy_draw_items: numberMetric(commandFamilyMetrics, "legacy_draw_items"),
         current_draw_pipeline_binds: numberMetric(commandFamilyMetrics, "current_draw_pipeline_binds"),
         legacy_draw_pipeline_binds: numberMetric(commandFamilyMetrics, "legacy_draw_pipeline_binds"),
         current_draw_bind_group_binds: numberMetric(commandFamilyMetrics, "current_draw_bind_group_binds"),
         legacy_draw_bind_group_binds: numberMetric(commandFamilyMetrics, "legacy_draw_bind_group_binds"),
         current_draw_scissor_sets: numberMetric(commandFamilyMetrics, "current_draw_scissor_sets"),
         legacy_draw_scissor_sets: numberMetric(commandFamilyMetrics, "legacy_draw_scissor_sets"),
         current_image_mesh_draws: numberMetric(commandFamilyMetrics, "current_image_mesh_draws"),
         legacy_image_mesh_draws: numberMetric(commandFamilyMetrics, "legacy_image_mesh_draws"),
         current_nine_slice_draws: numberMetric(commandFamilyMetrics, "current_nine_slice_draws"),
         legacy_nine_slice_draws: numberMetric(commandFamilyMetrics, "legacy_nine_slice_draws"),
         current_sdf_glyph_quads: numberMetric(commandFamilyMetrics, "current_sdf_glyph_quads"),
         legacy_sdf_glyph_quads: numberMetric(commandFamilyMetrics, "legacy_sdf_glyph_quads"),
         current_camera_bg_draws: numberMetric(commandFamilyMetrics, "current_camera_bg_draws"),
         legacy_camera_bg_draws: numberMetric(commandFamilyMetrics, "legacy_camera_bg_draws"),
         expected_image_meshes: numberMetric(commandFamilyMetrics, "expected_image_meshes"),
         expected_nine_slices: numberMetric(commandFamilyMetrics, "expected_nine_slices"),
         expected_sdf_glyphs: numberMetric(commandFamilyMetrics, "expected_sdf_glyphs"),
         expected_sdf_runs: numberMetric(commandFamilyMetrics, "expected_sdf_runs"),
         expected_camera_bg: numberMetric(commandFamilyMetrics, "expected_camera_bg"),
      },
      draw_state_summary: {
         id: "web.wasm.webgpu.draw_state_cache.current_vs_legacy_rebind",
         legacy_over_current: numberMetric(drawStateMetrics, "legacy_over_current"),
         current_p50_ms: numberMetric(drawStateMetrics, "current_p50_ms"),
         legacy_p50_ms: numberMetric(drawStateMetrics, "legacy_p50_ms"),
         current_draw_items: numberMetric(drawStateMetrics, "current_draw_items"),
         legacy_draw_items: numberMetric(drawStateMetrics, "legacy_draw_items"),
         current_draw_pipeline_binds: numberMetric(drawStateMetrics, "current_draw_pipeline_binds"),
         legacy_draw_pipeline_binds: numberMetric(drawStateMetrics, "legacy_draw_pipeline_binds"),
         current_draw_bind_group_binds: numberMetric(
            drawStateMetrics,
            "current_draw_bind_group_binds",
         ),
         legacy_draw_bind_group_binds: numberMetric(
            drawStateMetrics,
            "legacy_draw_bind_group_binds",
         ),
         current_draw_scissor_sets: numberMetric(drawStateMetrics, "current_draw_scissor_sets"),
         legacy_draw_scissor_sets: numberMetric(drawStateMetrics, "legacy_draw_scissor_sets"),
         expected_draw_items: numberMetric(drawStateMetrics, "expected_draw_items"),
      },
      clip_state_summary: {
         id: "web.wasm.webgpu.clip_state_cache.current_vs_legacy_rebind",
         legacy_over_current: numberMetric(clipStateMetrics, "legacy_over_current"),
         current_p50_ms: numberMetric(clipStateMetrics, "current_p50_ms"),
         legacy_p50_ms: numberMetric(clipStateMetrics, "legacy_p50_ms"),
         current_draw_items: numberMetric(clipStateMetrics, "current_draw_items"),
         legacy_draw_items: numberMetric(clipStateMetrics, "legacy_draw_items"),
         current_clip_depth_peak: numberMetric(clipStateMetrics, "current_clip_depth_peak"),
         legacy_clip_depth_peak: numberMetric(clipStateMetrics, "legacy_clip_depth_peak"),
         current_draw_pipeline_binds: numberMetric(clipStateMetrics, "current_draw_pipeline_binds"),
         legacy_draw_pipeline_binds: numberMetric(clipStateMetrics, "legacy_draw_pipeline_binds"),
         current_draw_bind_group_binds: numberMetric(
            clipStateMetrics,
            "current_draw_bind_group_binds",
         ),
         legacy_draw_bind_group_binds: numberMetric(
            clipStateMetrics,
            "legacy_draw_bind_group_binds",
         ),
         current_draw_scissor_sets: numberMetric(clipStateMetrics, "current_draw_scissor_sets"),
         legacy_draw_scissor_sets: numberMetric(clipStateMetrics, "legacy_draw_scissor_sets"),
         expected_draw_items: numberMetric(clipStateMetrics, "expected_draw_items"),
         expected_clip_runs: numberMetric(clipStateMetrics, "expected_clip_runs"),
         expected_clip_depth: numberMetric(clipStateMetrics, "expected_clip_depth"),
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
      lines.push(`| \`${row.id}\` | \`${row.variant}\` | ${row.samples} | ${row.frames_per_sample} | ${row.frames} | ${row.p50_ms.toFixed(3)} | ${row.p95_ms.toFixed(3)} | ${row.p99_ms.toFixed(3)} | ${row.peak_ms.toFixed(3)} | ${row.avg_ms.toFixed(3)} | ${row.unit} | \`${notes.join(";") || "-"}\` |`);
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
   lines.push("## Backend Path Coverage");
   lines.push("");
   lines.push("| Path | Status | Comparison | Rows | Counters |");
   lines.push("| --- | --- | --- | ---: | ---: |");
   for (let path of report.backend_path_coverage.paths) {
      lines.push(`| \`${path.id}\` | \`${path.status}\` | \`${path.comparison}\` | ${path.row_count} | ${path.counter_count} |`);
   }
   lines.push("");
   lines.push("## A/B Summary");
   lines.push("");
   lines.push("| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Passes | Legacy Passes | Current Upload Bytes | Legacy Upload Bytes | Vertices | Vertex Bytes |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.ab_summary.id}\` | ${report.ab_summary.current_p50_ms.toFixed(3)} | ${report.ab_summary.legacy_p50_ms.toFixed(3)} | ${report.ab_summary.legacy_over_current.toFixed(3)} | ${report.ab_summary.current_render_passes} | ${report.ab_summary.legacy_render_passes} | ${report.ab_summary.current_buffer_upload_bytes} | ${report.ab_summary.legacy_buffer_upload_bytes} | ${report.ab_summary.vertices} | ${report.ab_summary.vertex_bytes} |`);
   lines.push("");
   lines.push("## Upload Summary");
   lines.push("");
   lines.push("| Comparison | Glyph Current p50 ms | Glyph Legacy p50 ms | Glyph Legacy / Current | Glyph Current Texture Bytes | Glyph Legacy Texture Bytes | Glyph Current GPU ns | Glyph Legacy GPU ns | Glyph Legacy / Current GPU | Image Current p50 ms | Image Legacy p50 ms | Image Legacy / Current | Image Current Texture Bytes | Image Legacy Texture Bytes | Image Current GPU ns | Image Legacy GPU ns | Image Legacy / Current GPU |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.upload_summary.id}\` | ${report.upload_summary.glyph_current_p50_ms.toFixed(3)} | ${report.upload_summary.glyph_legacy_p50_ms.toFixed(3)} | ${report.upload_summary.glyph_legacy_over_current.toFixed(3)} | ${report.upload_summary.glyph_current_texture_upload_bytes} | ${report.upload_summary.glyph_legacy_texture_upload_bytes} | ${report.upload_summary.glyph_current_gpu_timestamp_total_ns} | ${report.upload_summary.glyph_legacy_gpu_timestamp_total_ns} | ${report.upload_summary.glyph_legacy_gpu_over_current.toFixed(3)} | ${report.upload_summary.image_current_p50_ms.toFixed(3)} | ${report.upload_summary.image_legacy_p50_ms.toFixed(3)} | ${report.upload_summary.image_legacy_over_current.toFixed(3)} | ${report.upload_summary.image_current_texture_upload_bytes} | ${report.upload_summary.image_legacy_texture_upload_bytes} | ${report.upload_summary.image_current_gpu_timestamp_total_ns} | ${report.upload_summary.image_legacy_gpu_timestamp_total_ns} | ${report.upload_summary.image_legacy_gpu_over_current.toFixed(3)} |`);
   lines.push("");
   lines.push("## Upload Scratch Summary");
   lines.push("");
   lines.push("| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Temp Allocs | Legacy Temp Allocs | Current Temp Bytes | Legacy Temp Bytes | Current Scratch Bytes | Legacy Scratch Bytes | Current Texture Bytes | Legacy Texture Bytes | Updates |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.upload_scratch_summary.id}\` | ${report.upload_scratch_summary.current_p50_ms.toFixed(3)} | ${report.upload_scratch_summary.legacy_p50_ms.toFixed(3)} | ${report.upload_scratch_summary.legacy_over_current.toFixed(3)} | ${report.upload_scratch_summary.current_temp_allocs} | ${report.upload_scratch_summary.legacy_temp_allocs} | ${report.upload_scratch_summary.current_temp_bytes} | ${report.upload_scratch_summary.legacy_temp_bytes} | ${report.upload_scratch_summary.current_scratch_bytes} | ${report.upload_scratch_summary.legacy_scratch_bytes} | ${report.upload_scratch_summary.current_texture_upload_bytes} | ${report.upload_scratch_summary.legacy_texture_upload_bytes} | ${report.upload_scratch_summary.updates} |`);
   lines.push("");
   lines.push("## Effect Uniform Summary");
   lines.push("");
   lines.push("| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current p50 | Current GPU ns | Legacy GPU ns | Legacy / Current GPU | Current Writes | Legacy Writes | Current Slots | Legacy Slots | Current Backdrops | Legacy Backdrops | Current Texture Copies | Legacy Texture Copies | Current Passes | Legacy Passes |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.effect_uniform_summary.id}\` | ${report.effect_uniform_summary.current_p50_ms.toFixed(3)} | ${report.effect_uniform_summary.legacy_p50_ms.toFixed(3)} | ${report.effect_uniform_summary.legacy_over_current.toFixed(3)} | ${report.effect_uniform_summary.current_gpu_timestamp_total_ns} | ${report.effect_uniform_summary.legacy_gpu_timestamp_total_ns} | ${report.effect_uniform_summary.legacy_gpu_over_current.toFixed(3)} | ${report.effect_uniform_summary.current_effect_uniform_writes} | ${report.effect_uniform_summary.legacy_effect_uniform_writes} | ${report.effect_uniform_summary.current_effect_uniform_slots} | ${report.effect_uniform_summary.legacy_effect_uniform_slots} | ${report.effect_uniform_summary.current_backdrop_draws} | ${report.effect_uniform_summary.legacy_backdrop_draws} | ${report.effect_uniform_summary.current_texture_copies} | ${report.effect_uniform_summary.legacy_texture_copies} | ${report.effect_uniform_summary.current_render_passes} | ${report.effect_uniform_summary.legacy_render_passes} |`);
   lines.push("");
   lines.push("## Backdrop Batch Summary");
   lines.push("");
   lines.push("| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Writes | Legacy Writes | Current Slots | Legacy Slots | Current Backdrops | Legacy Backdrops | Current Texture Copies | Legacy Texture Copies | Current Passes | Legacy Passes |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.backdrop_batch_summary.id}\` | ${report.backdrop_batch_summary.current_p50_ms.toFixed(3)} | ${report.backdrop_batch_summary.legacy_p50_ms.toFixed(3)} | ${report.backdrop_batch_summary.legacy_over_current.toFixed(3)} | ${report.backdrop_batch_summary.current_effect_uniform_writes} | ${report.backdrop_batch_summary.legacy_effect_uniform_writes} | ${report.backdrop_batch_summary.current_effect_uniform_slots} | ${report.backdrop_batch_summary.legacy_effect_uniform_slots} | ${report.backdrop_batch_summary.current_backdrop_draws} | ${report.backdrop_batch_summary.legacy_backdrop_draws} | ${report.backdrop_batch_summary.current_texture_copies} | ${report.backdrop_batch_summary.legacy_texture_copies} | ${report.backdrop_batch_summary.current_render_passes} | ${report.backdrop_batch_summary.legacy_render_passes} |`);
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
   lines.push("| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors | Current Writes | Legacy Writes | Current Texture Copies | Legacy Texture Copies | Current Passes | Legacy Passes |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.mixed_summary.id}\` | ${report.mixed_summary.current_p50_ms.toFixed(3)} | ${report.mixed_summary.legacy_p50_ms.toFixed(3)} | ${report.mixed_summary.legacy_over_current.toFixed(3)} | ${report.mixed_summary.current_draw_items} | ${report.mixed_summary.legacy_draw_items} | ${report.mixed_summary.current_draw_pipeline_binds} | ${report.mixed_summary.legacy_draw_pipeline_binds} | ${report.mixed_summary.current_draw_bind_group_binds} | ${report.mixed_summary.legacy_draw_bind_group_binds} | ${report.mixed_summary.current_draw_scissor_sets} | ${report.mixed_summary.legacy_draw_scissor_sets} | ${report.mixed_summary.current_effect_uniform_writes} | ${report.mixed_summary.legacy_effect_uniform_writes} | ${report.mixed_summary.current_texture_copies} | ${report.mixed_summary.legacy_texture_copies} | ${report.mixed_summary.current_render_passes} | ${report.mixed_summary.legacy_render_passes} |`);
   lines.push("");
   lines.push("## Layer Effects Summary");
   lines.push("");
   lines.push("| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors | Current Writes | Legacy Writes | Current Texture Copies | Legacy Texture Copies | Current Passes | Legacy Passes |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.layer_effects_summary.id}\` | ${report.layer_effects_summary.current_p50_ms.toFixed(3)} | ${report.layer_effects_summary.legacy_p50_ms.toFixed(3)} | ${report.layer_effects_summary.legacy_over_current.toFixed(3)} | ${report.layer_effects_summary.current_draw_items} | ${report.layer_effects_summary.legacy_draw_items} | ${report.layer_effects_summary.current_draw_pipeline_binds} | ${report.layer_effects_summary.legacy_draw_pipeline_binds} | ${report.layer_effects_summary.current_draw_bind_group_binds} | ${report.layer_effects_summary.legacy_draw_bind_group_binds} | ${report.layer_effects_summary.current_draw_scissor_sets} | ${report.layer_effects_summary.legacy_draw_scissor_sets} | ${report.layer_effects_summary.current_effect_uniform_writes} | ${report.layer_effects_summary.legacy_effect_uniform_writes} | ${report.layer_effects_summary.current_texture_copies} | ${report.layer_effects_summary.legacy_texture_copies} | ${report.layer_effects_summary.current_render_passes} | ${report.layer_effects_summary.legacy_render_passes} |`);
   lines.push("");
   lines.push("## Command Family Summary");
   lines.push("");
   lines.push("| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors | Image Meshes | Nine Slices | SDF Glyphs | CameraBg Draws |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.command_family_summary.id}\` | ${report.command_family_summary.current_p50_ms.toFixed(3)} | ${report.command_family_summary.legacy_p50_ms.toFixed(3)} | ${report.command_family_summary.legacy_over_current.toFixed(3)} | ${report.command_family_summary.current_draw_items} | ${report.command_family_summary.legacy_draw_items} | ${report.command_family_summary.current_draw_pipeline_binds} | ${report.command_family_summary.legacy_draw_pipeline_binds} | ${report.command_family_summary.current_draw_bind_group_binds} | ${report.command_family_summary.legacy_draw_bind_group_binds} | ${report.command_family_summary.current_draw_scissor_sets} | ${report.command_family_summary.legacy_draw_scissor_sets} | ${report.command_family_summary.current_image_mesh_draws}/${report.command_family_summary.legacy_image_mesh_draws} | ${report.command_family_summary.current_nine_slice_draws}/${report.command_family_summary.legacy_nine_slice_draws} | ${report.command_family_summary.current_sdf_glyph_quads}/${report.command_family_summary.legacy_sdf_glyph_quads} | ${report.command_family_summary.current_camera_bg_draws}/${report.command_family_summary.legacy_camera_bg_draws} |`);
   lines.push("");
   lines.push("## Draw State Cache Summary");
   lines.push("");
   lines.push("| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.draw_state_summary.id}\` | ${report.draw_state_summary.current_p50_ms.toFixed(3)} | ${report.draw_state_summary.legacy_p50_ms.toFixed(3)} | ${report.draw_state_summary.legacy_over_current.toFixed(3)} | ${report.draw_state_summary.current_draw_items} | ${report.draw_state_summary.legacy_draw_items} | ${report.draw_state_summary.current_draw_pipeline_binds} | ${report.draw_state_summary.legacy_draw_pipeline_binds} | ${report.draw_state_summary.current_draw_bind_group_binds} | ${report.draw_state_summary.legacy_draw_bind_group_binds} | ${report.draw_state_summary.current_draw_scissor_sets} | ${report.draw_state_summary.legacy_draw_scissor_sets} |`);
   lines.push("");
   lines.push("## Clip State Cache Summary");
   lines.push("");
   lines.push("| Comparison | Current p50 ms | Legacy p50 ms | Legacy / Current | Current Items | Legacy Items | Current Clip Depth | Legacy Clip Depth | Current Pipeline Binds | Legacy Pipeline Binds | Current Bind Groups | Legacy Bind Groups | Current Scissors | Legacy Scissors |");
   lines.push("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
   lines.push(`| \`${report.clip_state_summary.id}\` | ${report.clip_state_summary.current_p50_ms.toFixed(3)} | ${report.clip_state_summary.legacy_p50_ms.toFixed(3)} | ${report.clip_state_summary.legacy_over_current.toFixed(3)} | ${report.clip_state_summary.current_draw_items} | ${report.clip_state_summary.legacy_draw_items} | ${report.clip_state_summary.current_clip_depth_peak} | ${report.clip_state_summary.legacy_clip_depth_peak} | ${report.clip_state_summary.current_draw_pipeline_binds} | ${report.clip_state_summary.legacy_draw_pipeline_binds} | ${report.clip_state_summary.current_draw_bind_group_binds} | ${report.clip_state_summary.legacy_draw_bind_group_binds} | ${report.clip_state_summary.current_draw_scissor_sets} | ${report.clip_state_summary.legacy_draw_scissor_sets} |`);
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
      "web.wasm.webgpu.frame_loop",
      "web.wasm.webgpu.id_mask_compositor.current",
      "web.wasm.webgpu.glyph_atlas_upload.current_dirty",
      "web.wasm.webgpu.image_upload.current_dirty",
      "web.wasm.webgpu.upload_scratch.current_reuse",
      "web.wasm.webgpu.effect_uniform.current_batched",
      "web.wasm.webgpu.backdrop_batch.current_coalesced",
      "web.wasm.webgpu.scene3d.reused_mesh",
      "web.wasm.webgpu.scene3d.stress_reused_mesh",
      "web.wasm.webgpu.mixed_text_image_effects",
      "web.wasm.webgpu.layer_damage_effects",
      "web.wasm.webgpu.command_family_matrix",
      "web.wasm.webgpu.draw_state_cache.current",
      "web.wasm.webgpu.clip_state_cache.current",
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
      throw new Error("web report contract found current-row WASM reallocations");
   }
   if (
      summary.max_wasm_allocs_per_frame > summary.budget_wasm_allocs_per_frame
      || summary.max_wasm_alloc_bytes_per_frame > summary.budget_wasm_alloc_bytes_per_frame
   ) {
      throw new Error("web report contract found WASM allocation budget regression");
   }
   let rowSet = new Set(rows);
   for (let id of [
      "web.wasm.webgpu.frame_loop",
      "web.wasm.webgpu.id_mask_compositor.current",
      "web.wasm.webgpu.glyph_atlas_upload.current_dirty",
      "web.wasm.webgpu.image_upload.current_dirty",
      "web.wasm.webgpu.upload_scratch.current_reuse",
      "web.wasm.webgpu.effect_uniform.current_batched",
      "web.wasm.webgpu.backdrop_batch.current_coalesced",
      "web.wasm.webgpu.scene3d.reused_mesh",
      "web.wasm.webgpu.scene3d.stress_reused_mesh",
      "web.wasm.webgpu.mixed_text_image_effects",
      "web.wasm.webgpu.layer_damage_effects",
      "web.wasm.webgpu.command_family_matrix",
      "web.wasm.webgpu.draw_state_cache.current",
      "web.wasm.webgpu.clip_state_cache.current",
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
      summary.status !== "shared-submit-boundary-profile"
      || summary.unique_signature_count !== 1
      || summary.checked_count !== audit.checked_count
      || summary.reference_row !== "web.wasm.webgpu.frame_loop"
   ) {
      throw new Error("web report contract found path-specific current-row WASM allocations");
   }
   let signatureRows = Array.isArray(summary.signature_rows) ? summary.signature_rows : [];
   if (signatureRows.length !== 1 || !Array.isArray(signatureRows[0].ids)) {
      throw new Error("web report contract has inconsistent WASM allocation invariance signatures");
   }
   if (signatureRows[0].ids.length !== audit.checked_count) {
      throw new Error("web report contract WASM allocation invariance does not cover every checked row");
   }
   let frame = audit.row_details.find(row => row.id === "web.wasm.webgpu.frame_loop");
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
      || summary.row_id !== "web.wasm.webgpu.frame_loop"
   ) {
      throw new Error("web report contract missing frame-loop WASM allocation stage summary");
   }
   let frame = byId.get("web.wasm.webgpu.frame_loop");
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
            if (detail[field] !== row[field]) {
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
      "web.wasm.webgpu.frame_loop",
      "web.wasm.webgpu.id_mask_compositor.current",
      "web.wasm.webgpu.id_mask_compositor.legacy_upload",
      "web.wasm.webgpu.glyph_atlas_upload.current_dirty",
      "web.wasm.webgpu.glyph_atlas_upload.legacy_full",
      "web.wasm.webgpu.image_upload.current_dirty",
      "web.wasm.webgpu.image_upload.legacy_full",
      "web.wasm.webgpu.upload_scratch.current_reuse",
      "web.wasm.webgpu.upload_scratch.legacy_temp_alloc",
      "web.wasm.webgpu.effect_uniform.current_batched",
      "web.wasm.webgpu.effect_uniform.legacy_write_each",
      "web.wasm.webgpu.backdrop_batch.current_coalesced",
      "web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy",
      "web.wasm.webgpu.scene3d.reused_mesh",
      "web.wasm.webgpu.scene3d.recreate_mesh",
      "web.wasm.webgpu.scene3d.stress_reused_mesh",
      "web.wasm.webgpu.scene3d.stress_recreate_mesh",
      "web.wasm.webgpu.mixed_text_image_effects",
      "web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched",
      "web.wasm.webgpu.layer_damage_effects",
      "web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched",
      "web.wasm.webgpu.command_family_matrix",
      "web.wasm.webgpu.command_family_matrix.legacy_rebind",
      "web.wasm.webgpu.draw_state_cache.current",
      "web.wasm.webgpu.draw_state_cache.legacy_rebind",
      "web.wasm.webgpu.clip_state_cache.current",
      "web.wasm.webgpu.clip_state_cache.legacy_rebind",
   ]);
   let cpuScratchGrowthAllowed = new Set([
      "web.wasm.webgpu.scene3d.recreate_mesh",
      "web.wasm.webgpu.scene3d.stress_recreate_mesh",
   ]);
   for (let row of report.cases) {
      expected.delete(row.id);
      for (let key of [
         "samples",
         "frames_per_sample",
         "frames",
         "p50_ms",
         "p95_ms",
         "p99_ms",
         "peak_ms",
         "avg_ms",
         "frame_budget_60hz_ms",
         "missed_frame_ratio_60hz",
         "hitch_ratio_60hz",
         "frame_budget_120hz_ms",
         "missed_frame_ratio_120hz",
         "hitch_ratio_120hz",
         "draws",
         "draw_items",
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
   assertBenchmarkMarks(report);
   let byId = new Map(report.cases.map(row => [row.id, row]));
   assertWarmResourceChurn(report, byId);
   assertWasmAllocationAudit(report, byId);
   assertWasmAllocationInvariance(report);
   assertFrameLoopWasmStageAllocation(report, byId);
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
   let mixedLegacy = byId.get("web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched");
   if (
      mixed.backdrop_draws <= 0
      || mixedLegacy.backdrop_draws <= 0
      || mixed.visual_effect_draws <= 0
      || mixedLegacy.visual_effect_draws <= 0
      || mixed.layer_draws <= 0
      || mixedLegacy.layer_draws <= 0
      || mixed.damage_rects <= 0
      || mixedLegacy.damage_rects <= 0
      || mixed.clip_depth_peak <= 0
      || mixedLegacy.clip_depth_peak <= 0
   ) {
      throw new Error("mixed WebGPU A/B rows must cover backdrop, visual effect, layer, clip, and damage counters");
   }
   if (
      mixed.draw_items !== mixedLegacy.draw_items
      || mixed.glyph_quads !== mixedLegacy.glyph_quads
      || mixed.image_draws !== mixedLegacy.image_draws
      || mixed.image_draws < mixed.image_tiles
      || mixedLegacy.image_draws < mixedLegacy.image_tiles
      || mixed.backdrop_draws !== mixedLegacy.backdrop_draws
      || mixed.visual_effect_draws !== mixedLegacy.visual_effect_draws
      || mixed.spinner_draws !== mixedLegacy.spinner_draws
      || mixed.layer_draws !== mixedLegacy.layer_draws
      || mixed.damage_rects !== mixedLegacy.damage_rects
      || mixed.draw_pipeline_binds >= mixedLegacy.draw_pipeline_binds
      || mixed.draw_bind_group_binds > mixedLegacy.draw_bind_group_binds
      || mixed.draw_scissor_sets >= mixedLegacy.draw_scissor_sets
      || mixed.effect_uniform_writes >= mixedLegacy.effect_uniform_writes
      || mixed.texture_copies > mixedLegacy.texture_copies
      || mixed.render_passes > mixedLegacy.render_passes
      || mixed.gpu_timestamp_passes !== mixed.render_passes
      || mixedLegacy.gpu_timestamp_passes !== mixedLegacy.render_passes
   ) {
      throw new Error(
         "mixed WebGPU A/B rows must prove equivalent visible work, fewer current pipeline/scissor/effect writes, no extra bind groups, and timestamped passes: "
            + `items=${mixed.draw_items}/${mixedLegacy.draw_items} `
            + `glyphs=${mixed.glyph_quads}/${mixedLegacy.glyph_quads} `
            + `images=${mixed.image_draws}/${mixedLegacy.image_draws} tiles=${mixed.image_tiles}/${mixedLegacy.image_tiles} `
            + `backdrops=${mixed.backdrop_draws}/${mixedLegacy.backdrop_draws} `
            + `visual_effects=${mixed.visual_effect_draws}/${mixedLegacy.visual_effect_draws} `
            + `layers=${mixed.layer_draws}/${mixedLegacy.layer_draws} `
            + `damage=${mixed.damage_rects}/${mixedLegacy.damage_rects} `
            + `pipeline_binds=${mixed.draw_pipeline_binds}/${mixedLegacy.draw_pipeline_binds} `
            + `bind_groups=${mixed.draw_bind_group_binds}/${mixedLegacy.draw_bind_group_binds} `
            + `scissors=${mixed.draw_scissor_sets}/${mixedLegacy.draw_scissor_sets} `
            + `effect_writes=${mixed.effect_uniform_writes}/${mixedLegacy.effect_uniform_writes} `
            + `copies=${mixed.texture_copies}/${mixedLegacy.texture_copies} `
            + `passes=${mixed.render_passes}/${mixedLegacy.render_passes} `
            + `timestamp_passes=${mixed.gpu_timestamp_passes}/${mixedLegacy.gpu_timestamp_passes}`
      );
   }
   if (
      report.mixed_summary.current_p50_ms !== mixed.p50_ms
      || report.mixed_summary.legacy_p50_ms !== mixedLegacy.p50_ms
      || report.mixed_summary.current_draw_pipeline_binds !== mixed.draw_pipeline_binds
      || report.mixed_summary.legacy_draw_pipeline_binds !== mixedLegacy.draw_pipeline_binds
      || report.mixed_summary.current_draw_bind_group_binds !== mixed.draw_bind_group_binds
      || report.mixed_summary.legacy_draw_bind_group_binds !== mixedLegacy.draw_bind_group_binds
      || report.mixed_summary.current_draw_scissor_sets !== mixed.draw_scissor_sets
      || report.mixed_summary.legacy_draw_scissor_sets !== mixedLegacy.draw_scissor_sets
      || report.mixed_summary.current_effect_uniform_writes !== mixed.effect_uniform_writes
      || report.mixed_summary.legacy_effect_uniform_writes !== mixedLegacy.effect_uniform_writes
   ) {
      throw new Error("mixed WebGPU summary must match current and legacy source rows");
   }
   let layerEffects = byId.get("web.wasm.webgpu.layer_damage_effects");
   let layerEffectsLegacy = byId.get("web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched");
   if (
      layerEffects.layer_draws < layerEffects.expected_layers
      || layerEffectsLegacy.layer_draws < layerEffectsLegacy.expected_layers
      || layerEffects.damage_rects < layerEffects.expected_damage_rects
      || layerEffectsLegacy.damage_rects < layerEffectsLegacy.expected_damage_rects
      || layerEffects.clip_depth_peak <= 0
      || layerEffectsLegacy.clip_depth_peak <= 0
      || layerEffects.backdrop_draws < layerEffects.expected_backdrops
      || layerEffectsLegacy.backdrop_draws < layerEffectsLegacy.expected_backdrops
      || layerEffects.visual_effect_draws <= 0
      || layerEffectsLegacy.visual_effect_draws <= 0
      || layerEffects.spinner_draws <= 0
      || layerEffectsLegacy.spinner_draws <= 0
      || layerEffects.texture_copies <= 0
      || layerEffectsLegacy.texture_copies <= 0
      || layerEffects.gpu_timestamp_passes !== layerEffects.render_passes
      || layerEffectsLegacy.gpu_timestamp_passes !== layerEffectsLegacy.render_passes
   ) {
      throw new Error("layer/effects WebGPU A/B rows must cover layers, damage, clips, effects, texture copies, spinner work, and timestamped passes");
   }
   if (
      layerEffects.draw_items !== layerEffectsLegacy.draw_items
      || layerEffects.glyph_quads !== layerEffectsLegacy.glyph_quads
      || layerEffects.image_draws !== layerEffectsLegacy.image_draws
      || layerEffects.image_draws < layerEffects.image_tiles
      || layerEffectsLegacy.image_draws < layerEffectsLegacy.image_tiles
      || layerEffects.layer_draws !== layerEffectsLegacy.layer_draws
      || layerEffects.damage_rects !== layerEffectsLegacy.damage_rects
      || layerEffects.backdrop_draws !== layerEffectsLegacy.backdrop_draws
      || layerEffects.visual_effect_draws !== layerEffectsLegacy.visual_effect_draws
      || layerEffects.spinner_draws !== layerEffectsLegacy.spinner_draws
      || layerEffects.draw_pipeline_binds >= layerEffectsLegacy.draw_pipeline_binds
      || layerEffects.draw_bind_group_binds >= layerEffectsLegacy.draw_bind_group_binds
      || layerEffects.draw_scissor_sets >= layerEffectsLegacy.draw_scissor_sets
      || layerEffects.effect_uniform_writes >= layerEffectsLegacy.effect_uniform_writes
      || layerEffects.texture_copies >= layerEffectsLegacy.texture_copies
      || layerEffects.render_passes >= layerEffectsLegacy.render_passes
   ) {
      throw new Error(
         "layer/effects WebGPU A/B rows must prove equivalent visible work and fewer current state/effect/pass operations: "
            + `items=${layerEffects.draw_items}/${layerEffectsLegacy.draw_items} `
            + `glyphs=${layerEffects.glyph_quads}/${layerEffectsLegacy.glyph_quads} `
            + `images=${layerEffects.image_draws}/${layerEffectsLegacy.image_draws} tiles=${layerEffects.image_tiles}/${layerEffectsLegacy.image_tiles} `
            + `layers=${layerEffects.layer_draws}/${layerEffectsLegacy.layer_draws} `
            + `damage=${layerEffects.damage_rects}/${layerEffectsLegacy.damage_rects} `
            + `backdrops=${layerEffects.backdrop_draws}/${layerEffectsLegacy.backdrop_draws} `
            + `visual_effects=${layerEffects.visual_effect_draws}/${layerEffectsLegacy.visual_effect_draws} `
            + `spinners=${layerEffects.spinner_draws}/${layerEffectsLegacy.spinner_draws} `
            + `pipeline_binds=${layerEffects.draw_pipeline_binds}/${layerEffectsLegacy.draw_pipeline_binds} `
            + `bind_groups=${layerEffects.draw_bind_group_binds}/${layerEffectsLegacy.draw_bind_group_binds} `
            + `scissors=${layerEffects.draw_scissor_sets}/${layerEffectsLegacy.draw_scissor_sets} `
            + `effect_writes=${layerEffects.effect_uniform_writes}/${layerEffectsLegacy.effect_uniform_writes} `
            + `copies=${layerEffects.texture_copies}/${layerEffectsLegacy.texture_copies} `
            + `passes=${layerEffects.render_passes}/${layerEffectsLegacy.render_passes}`
      );
   }
   if (
      report.layer_effects_summary.current_p50_ms !== layerEffects.p50_ms
      || report.layer_effects_summary.legacy_p50_ms !== layerEffectsLegacy.p50_ms
      || report.layer_effects_summary.current_draw_pipeline_binds !== layerEffects.draw_pipeline_binds
      || report.layer_effects_summary.legacy_draw_pipeline_binds !== layerEffectsLegacy.draw_pipeline_binds
      || report.layer_effects_summary.current_draw_bind_group_binds !== layerEffects.draw_bind_group_binds
      || report.layer_effects_summary.legacy_draw_bind_group_binds !== layerEffectsLegacy.draw_bind_group_binds
      || report.layer_effects_summary.current_draw_scissor_sets !== layerEffects.draw_scissor_sets
      || report.layer_effects_summary.legacy_draw_scissor_sets !== layerEffectsLegacy.draw_scissor_sets
      || report.layer_effects_summary.current_effect_uniform_writes !== layerEffects.effect_uniform_writes
      || report.layer_effects_summary.legacy_effect_uniform_writes !== layerEffectsLegacy.effect_uniform_writes
   ) {
      throw new Error("layer/effects WebGPU summary must match current and legacy source rows");
   }
   if (report.layer_effects_summary.legacy_over_current <= 1.0) {
      throw new Error(
         `layer/effects current row must beat legacy rebind/unbatched p50: current=${report.layer_effects_summary.current_p50_ms.toFixed(3)}ms legacy=${report.layer_effects_summary.legacy_p50_ms.toFixed(3)}ms ratio=${report.layer_effects_summary.legacy_over_current.toFixed(3)}`
      );
   }
   let commandFamily = byId.get("web.wasm.webgpu.command_family_matrix");
   let commandFamilyLegacy = byId.get("web.wasm.webgpu.command_family_matrix.legacy_rebind");
   if (
      commandFamily.image_mesh_draws < commandFamily.expected_image_meshes
      || commandFamilyLegacy.image_mesh_draws < commandFamilyLegacy.expected_image_meshes
      || commandFamily.nine_slice_draws < commandFamily.expected_nine_slices
      || commandFamilyLegacy.nine_slice_draws < commandFamilyLegacy.expected_nine_slices
      || commandFamily.sdf_glyph_quads < commandFamily.expected_sdf_glyphs
      || commandFamilyLegacy.sdf_glyph_quads < commandFamilyLegacy.expected_sdf_glyphs
      || commandFamily.expected_camera_bg !== 0
      || commandFamilyLegacy.expected_camera_bg !== 0
      || commandFamily.camera_bg_draws !== 0
      || commandFamilyLegacy.camera_bg_draws !== 0
      || commandFamily.gpu_timestamp_passes !== commandFamily.render_passes
      || commandFamilyLegacy.gpu_timestamp_passes !== commandFamilyLegacy.render_passes
   ) {
      throw new Error("command-family WebGPU A/B rows must cover ImageMesh, NineSlice, SDF glyph, zero web CameraBg, and timestamped passes");
   }
   if (
      commandFamily.draw_items !== commandFamilyLegacy.draw_items
      || commandFamily.image_mesh_draws !== commandFamilyLegacy.image_mesh_draws
      || commandFamily.nine_slice_draws !== commandFamilyLegacy.nine_slice_draws
      || commandFamily.sdf_glyph_quads !== commandFamilyLegacy.sdf_glyph_quads
      || commandFamily.camera_bg_draws !== commandFamilyLegacy.camera_bg_draws
      || commandFamily.draw_pipeline_binds >= commandFamilyLegacy.draw_pipeline_binds
      || commandFamily.draw_bind_group_binds >= commandFamilyLegacy.draw_bind_group_binds
      || commandFamily.draw_scissor_sets >= commandFamilyLegacy.draw_scissor_sets
   ) {
      throw new Error(
         "command-family WebGPU A/B rows must prove equivalent generic work, zero web CameraBg, and fewer current state binds: "
            + `items=${commandFamily.draw_items}/${commandFamilyLegacy.draw_items} `
            + `image_meshes=${commandFamily.image_mesh_draws}/${commandFamilyLegacy.image_mesh_draws} `
            + `nine_slices=${commandFamily.nine_slice_draws}/${commandFamilyLegacy.nine_slice_draws} `
            + `sdf=${commandFamily.sdf_glyph_quads}/${commandFamilyLegacy.sdf_glyph_quads} `
            + `camera_bg=${commandFamily.camera_bg_draws}/${commandFamilyLegacy.camera_bg_draws} `
            + `pipeline_binds=${commandFamily.draw_pipeline_binds}/${commandFamilyLegacy.draw_pipeline_binds} `
            + `bind_groups=${commandFamily.draw_bind_group_binds}/${commandFamilyLegacy.draw_bind_group_binds} `
            + `scissors=${commandFamily.draw_scissor_sets}/${commandFamilyLegacy.draw_scissor_sets}`
      );
   }
   if (
      report.command_family_summary.current_p50_ms !== commandFamily.p50_ms
      || report.command_family_summary.legacy_p50_ms !== commandFamilyLegacy.p50_ms
      || report.command_family_summary.current_draw_pipeline_binds !== commandFamily.draw_pipeline_binds
      || report.command_family_summary.legacy_draw_pipeline_binds !== commandFamilyLegacy.draw_pipeline_binds
      || report.command_family_summary.current_draw_bind_group_binds !== commandFamily.draw_bind_group_binds
      || report.command_family_summary.legacy_draw_bind_group_binds !== commandFamilyLegacy.draw_bind_group_binds
      || report.command_family_summary.current_draw_scissor_sets !== commandFamily.draw_scissor_sets
      || report.command_family_summary.legacy_draw_scissor_sets !== commandFamilyLegacy.draw_scissor_sets
   ) {
      throw new Error("command-family WebGPU summary must match current and legacy source rows");
   }
   if (report.command_family_summary.legacy_over_current <= 1.0) {
      throw new Error(
         `command-family current row must beat legacy rebind p50: current=${report.command_family_summary.current_p50_ms.toFixed(3)}ms legacy=${report.command_family_summary.legacy_p50_ms.toFixed(3)}ms ratio=${report.command_family_summary.legacy_over_current.toFixed(3)}`
      );
   }
   let glyphUploadCurrent = byId.get("web.wasm.webgpu.glyph_atlas_upload.current_dirty");
   let glyphUploadLegacy = byId.get("web.wasm.webgpu.glyph_atlas_upload.legacy_full");
   let imageUploadCurrent = byId.get("web.wasm.webgpu.image_upload.current_dirty");
   let imageUploadLegacy = byId.get("web.wasm.webgpu.image_upload.legacy_full");
   if (
      glyphUploadCurrent.glyph_quads <= 0
      || glyphUploadLegacy.glyph_quads <= 0
      || glyphUploadCurrent.texture_upload_bytes >= glyphUploadLegacy.texture_upload_bytes
      || glyphUploadCurrent.gpu_timestamp_passes !== glyphUploadCurrent.render_passes
      || glyphUploadLegacy.gpu_timestamp_passes !== glyphUploadLegacy.render_passes
      || glyphUploadCurrent.gpu_timestamp_total_ns <= 0
      || glyphUploadLegacy.gpu_timestamp_total_ns <= 0
      || report.upload_summary.glyph_current_gpu_timestamp_total_ns !== glyphUploadCurrent.gpu_timestamp_total_ns
      || report.upload_summary.glyph_legacy_gpu_timestamp_total_ns !== glyphUploadLegacy.gpu_timestamp_total_ns
   ) {
      throw new Error("glyph upload WebGPU A/B rows must prove dirty atlas upload, lower current bytes, and timestamped passes");
   }
   if (
      imageUploadCurrent.image_draws <= 0
      || imageUploadLegacy.image_draws <= 0
      || imageUploadCurrent.texture_upload_bytes >= imageUploadLegacy.texture_upload_bytes
      || imageUploadCurrent.gpu_timestamp_passes !== imageUploadCurrent.render_passes
      || imageUploadLegacy.gpu_timestamp_passes !== imageUploadLegacy.render_passes
      || imageUploadCurrent.gpu_timestamp_total_ns <= 0
      || imageUploadLegacy.gpu_timestamp_total_ns <= 0
      || report.upload_summary.image_current_gpu_timestamp_total_ns !== imageUploadCurrent.gpu_timestamp_total_ns
      || report.upload_summary.image_legacy_gpu_timestamp_total_ns !== imageUploadLegacy.gpu_timestamp_total_ns
   ) {
      throw new Error("image upload WebGPU A/B rows must prove dirty RGBA upload, lower current bytes, and timestamped passes");
   }
   if (report.upload_summary.glyph_legacy_over_current <= 1.0 || report.upload_summary.image_legacy_over_current <= 1.0) {
      throw new Error("upload current rows must beat legacy full-upload rows in browser p50");
   }
   let uploadScratchCurrent = byId.get("web.wasm.webgpu.upload_scratch.current_reuse");
   let uploadScratchLegacy = byId.get("web.wasm.webgpu.upload_scratch.legacy_temp_alloc");
   if (
      uploadScratchCurrent.image_upload_temp_allocs !== 0
      || uploadScratchCurrent.image_upload_temp_bytes !== 0
      || uploadScratchLegacy.image_upload_temp_allocs <= 0
      || uploadScratchLegacy.image_upload_temp_bytes <= 0
      || uploadScratchCurrent.texture_upload_bytes !== uploadScratchLegacy.texture_upload_bytes
      || uploadScratchCurrent.image_draws <= 0
      || uploadScratchCurrent.glyph_quads <= 0
      || uploadScratchCurrent.gpu_timestamp_passes !== uploadScratchCurrent.render_passes
      || uploadScratchLegacy.gpu_timestamp_passes !== uploadScratchLegacy.render_passes
   ) {
      throw new Error("upload-scratch WebGPU A/B rows must prove current zero temp allocations with equivalent upload work");
   }
   if (report.upload_scratch_summary.legacy_over_current <= 1.0) {
      throw new Error(
         `upload-scratch current row must beat legacy temp allocation p50: current=${report.upload_scratch_summary.current_p50_ms.toFixed(3)}ms legacy=${report.upload_scratch_summary.legacy_p50_ms.toFixed(3)}ms ratio=${report.upload_scratch_summary.legacy_over_current.toFixed(3)}`
      );
   }
   let effectCurrent = byId.get("web.wasm.webgpu.effect_uniform.current_batched");
   let effectLegacy = byId.get("web.wasm.webgpu.effect_uniform.legacy_write_each");
   if (
      effectCurrent.backdrop_draws < effectCurrent.expected_backdrops
      || effectLegacy.backdrop_draws < effectLegacy.expected_backdrops
      || effectCurrent.effect_uniform_writes !== 1
      || effectLegacy.effect_uniform_writes <= effectCurrent.effect_uniform_writes
      || effectCurrent.effect_uniform_slots !== effectCurrent.expected_backdrops
      || effectLegacy.effect_uniform_slots !== effectLegacy.expected_backdrops
      || effectCurrent.texture_copies !== effectLegacy.texture_copies
      || effectCurrent.render_passes !== effectLegacy.render_passes
      || effectCurrent.gpu_timestamp_passes !== effectCurrent.render_passes
      || effectLegacy.gpu_timestamp_passes !== effectLegacy.render_passes
   ) {
      throw new Error("effect-uniform WebGPU A/B rows must prove one batched current write, equivalent effects, and timestamped passes");
   }
   let backdropBatchCurrent = byId.get("web.wasm.webgpu.backdrop_batch.current_coalesced");
   let backdropBatchLegacy = byId.get("web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy");
   if (
      backdropBatchCurrent.backdrop_draws < backdropBatchCurrent.expected_backdrops
      || backdropBatchLegacy.backdrop_draws < backdropBatchLegacy.expected_backdrops
      || backdropBatchCurrent.effect_uniform_writes !== backdropBatchLegacy.effect_uniform_writes
      || backdropBatchCurrent.effect_uniform_slots !== backdropBatchLegacy.effect_uniform_slots
      || backdropBatchCurrent.effect_uniform_slots !== backdropBatchCurrent.expected_backdrops
      || backdropBatchCurrent.texture_copies >= backdropBatchLegacy.texture_copies
      || backdropBatchCurrent.render_passes >= backdropBatchLegacy.render_passes
      || backdropBatchCurrent.gpu_timestamp_passes !== backdropBatchCurrent.render_passes
      || backdropBatchLegacy.gpu_timestamp_passes !== backdropBatchLegacy.render_passes
   ) {
      throw new Error("backdrop-batch WebGPU A/B rows must prove equivalent effects, fewer current texture copies/render passes, and timestamped passes");
   }
   if (report.backdrop_batch_summary.legacy_over_current <= 1.0) {
      throw new Error(
         `backdrop-batch current row must beat legacy per-backdrop copy p50: current=${report.backdrop_batch_summary.current_p50_ms.toFixed(3)}ms legacy=${report.backdrop_batch_summary.legacy_p50_ms.toFixed(3)}ms ratio=${report.backdrop_batch_summary.legacy_over_current.toFixed(3)}`
      );
   }
   let drawStateCurrent = byId.get("web.wasm.webgpu.draw_state_cache.current");
   let drawStateLegacy = byId.get("web.wasm.webgpu.draw_state_cache.legacy_rebind");
   if (
      drawStateCurrent.draw_items < drawStateCurrent.expected_draw_items
      || drawStateLegacy.draw_items < drawStateLegacy.expected_draw_items
      || drawStateCurrent.draws !== drawStateCurrent.draw_items
      || drawStateLegacy.draws !== drawStateLegacy.draw_items
      || drawStateCurrent.draw_pipeline_binds >= drawStateLegacy.draw_pipeline_binds
      || drawStateCurrent.draw_bind_group_binds >= drawStateLegacy.draw_bind_group_binds
      || drawStateCurrent.draw_scissor_sets >= drawStateLegacy.draw_scissor_sets
      || drawStateCurrent.gpu_timestamp_passes !== drawStateCurrent.render_passes
      || drawStateLegacy.gpu_timestamp_passes !== drawStateLegacy.render_passes
   ) {
      throw new Error("draw-state cache WebGPU A/B rows must prove equivalent draw count, fewer current state binds, and timestamped passes");
   }
   if (report.draw_state_summary.legacy_over_current <= 1.0) {
      throw new Error("draw-state cache current row must beat legacy rebind p50");
   }
   let clipStateCurrent = byId.get("web.wasm.webgpu.clip_state_cache.current");
   let clipStateLegacy = byId.get("web.wasm.webgpu.clip_state_cache.legacy_rebind");
   if (
      clipStateCurrent.draw_items < clipStateCurrent.expected_draw_items
      || clipStateLegacy.draw_items < clipStateLegacy.expected_draw_items
      || clipStateCurrent.draws !== clipStateCurrent.draw_items
      || clipStateLegacy.draws !== clipStateLegacy.draw_items
      || clipStateCurrent.clip_depth_peak < clipStateCurrent.expected_clip_depth
      || clipStateLegacy.clip_depth_peak < clipStateLegacy.expected_clip_depth
      || clipStateCurrent.draw_pipeline_binds >= clipStateLegacy.draw_pipeline_binds
      || clipStateCurrent.draw_bind_group_binds >= clipStateLegacy.draw_bind_group_binds
      || clipStateCurrent.draw_scissor_sets > clipStateCurrent.expected_clip_runs
      || clipStateCurrent.draw_scissor_sets >= clipStateLegacy.draw_scissor_sets
      || clipStateCurrent.gpu_timestamp_passes !== clipStateCurrent.render_passes
      || clipStateLegacy.gpu_timestamp_passes !== clipStateLegacy.render_passes
   ) {
      throw new Error("clip-state cache WebGPU A/B rows must prove equivalent clipped draws, nonzero clip depth, fewer current state binds/scissors, and timestamped passes");
   }
   if (report.clip_state_summary.legacy_over_current <= 1.0) {
      throw new Error("clip-state cache current row must beat legacy rebind p50");
   }
}

function writeWebReports(args, url, pageReport, pixelReport, traceSummary)
{
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

async function main()
{
   let args = parseArgs(process.argv.slice(2));
   let tempDir = mkdtempSync(join(tmpdir(), "oxide-webgpu-golden-"));
   let defaultOutName =
      args.target === "id-mask"
         ? "webgpu_id_mask_compositor.png"
         : args.target === "scene3d"
           ? "webgpu_scene3d.png"
           : "webgpu_browser.png";
   let out = args.out || join(tempDir, defaultOutName);
   mkdirSync(dirname(out), { recursive: true });

   let { server, nextReportPromise } = await startServer();
   let address = server.address();
   let url = `http://127.0.0.1:${address.port}/`;
   let captureUrl = browserUrl(args, url, false);
   let browserReportUrl = browserUrl(args, url, true);
   try {
      let { capture, diff } = await captureAndCompare(args, captureUrl, out);
      if (args.jsonReport || args.markdownReport) {
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
