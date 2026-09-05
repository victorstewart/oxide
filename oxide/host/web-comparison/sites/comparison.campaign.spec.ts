import {expect, test, chromium, type BrowserContext, type CDPSession, type Page} from "@playwright/test";
import {createHash} from "node:crypto";
import {createRequire} from "node:module";
import {mkdir, readFile, rename, writeFile} from "node:fs/promises";
import path from "node:path";
import {gunzipSync} from "node:zlib";
import {PNG} from "pngjs";

type DecimalU64 = string;

type ImplementationIdentity = {
   id: string;
   variant: string;
   shipping_payload_manifest_sha256: string;
};

type ShippingTotals = {
   raw_bytes: number;
   gzip_9_bytes: number;
   brotli_11_bytes: number;
   file_count: number;
};

type ShippingManifest = {
   schema_version: number;
   implementation_id: string;
   category_totals: Record<string, ShippingTotals>;
   route_class_totals: Record<string, ShippingTotals>;
   totals: ShippingTotals;
};

type ComparisonCell = {
   scenario_id: string;
   cache_class: string;
   pack_id: string;
};

type ComparisonPlan = {
   schema_version: number;
   suite_id: string;
   plan_id: string;
   plan_sha256: string;
   platform: "web";
   reference: ImplementationIdentity;
   contender: ImplementationIdentity;
   common: {
      harness_sha256: string;
      pass_instrumentation_sha256: string;
   };
   environment: {chrome_version: string};
   seed: DecimalU64;
   scenario_ids: string[];
   scenario_packs: {id: string; ordered_scenario_ids: string[]}[];
   controller_chunks: {id: string; ordered_pair_indices: DecimalU64[]; pack_ids: string[]; pass_id: string; checkpoint_generation: DecimalU64}[];
   comparison_cells: ComparisonCell[];
};

type NetworkRow = {
   request_id: string;
   url: string;
   mime_type: string | null;
   status: number | null;
   protocol: string | null;
   encoded_data_length: number | null;
   from_disk_cache: boolean | null;
   from_prefetch_cache: boolean | null;
   from_service_worker: boolean | null;
};

type TraceCompletion = {
   dataLossOccurred: boolean;
   stream?: string;
   traceFormat?: string;
   streamCompression?: string;
};

const sourceRoot = path.resolve(import.meta.dirname);
const harnessRoot = path.dirname(sourceRoot);
const planPath = process.env.OXIDE_COMPARISON_PLAN;
const resultPath = process.env.OXIDE_COMPARISON_RESULT;
if (!planPath || !resultPath) {
   throw new Error("OXIDE_COMPARISON_PLAN and OXIDE_COMPARISON_RESULT are required");
}
const plan = JSON.parse(await readFile(planPath, "utf8")) as ComparisonPlan;
const requiredChromeVersion = plan.environment?.chrome_version;
if (plan.schema_version !== 1 || plan.platform !== "web" || !requiredChromeVersion) {
   throw new Error(`unsupported comparison plan ${plan.plan_id}`);
}

const require = createRequire(import.meta.url);
const webVitalsPath = require.resolve("web-vitals");
const expectedWebVitalsSha256 = "d4227a2ad276d3c60e10e80abf54f71676c322558ac3dda42d4b4f71c73d8f83";
if (path.basename(webVitalsPath) !== "web-vitals.umd.cjs") {
   throw new Error(`web-vitals must resolve to its classic-script UMD build, received ${webVitalsPath}`);
}
const webVitalsSource = await readFile(webVitalsPath, "utf8");
const observerSource = await readFile(path.join(sourceRoot, "observer.init.js"), "utf8");
const commonInitSource = `${webVitalsSource}\n;${observerSource}`;
const webVitalsSha256 = createHash("sha256").update(webVitalsSource).digest("hex");
const passInstrumentationSha256 = createHash("sha256").update(commonInitSource).digest("hex");
const harnessSha256 = createHash("sha256").update(await readFile(path.join(sourceRoot, "comparison.campaign.spec.ts"))).digest("hex");
if (webVitalsSha256 !== expectedWebVitalsSha256) {
   throw new Error(`web-vitals runtime hash mismatch: ${webVitalsSha256}`);
}
if (passInstrumentationSha256 !== plan.common.pass_instrumentation_sha256) {
   throw new Error(`comparison pass instrumentation hash mismatch: ${passInstrumentationSha256}`);
}
if (harnessSha256 !== plan.common.harness_sha256) {
   throw new Error(`comparison campaign hash mismatch: ${harnessSha256}`);
}

const baseUrl = process.env.OXIDE_COMPARISON_ORIGIN || "http://127.0.0.1:4173";
const headed = true;
const completedPairs: unknown[] = [];
const verifiedShippingManifests: unknown[] = [];
const eligibility = {
   authoritative: false,
   classification: "correctness-only-visual-parity-pending",
   blocking_reasons: [
      "headed exact full-frame opaque-sRGB8 pixel acceptance has not yet completed",
      "common Chromium compositor-presentation correlation has not yet been acquired and calibrated",
      "the in-app browser must execute and retain the paired campaign evidence",
   ],
};
test.use({headless: !headed, viewport: {width: 390, height: 844}, deviceScaleFactor: 3});

const implementationRoute = (identity: ImplementationIdentity) => {
   const route = identity.id.split(".")[0];
   if (!["html-first", "client-dom", "oxide"].includes(route)) {
      throw new Error(`unsupported browser implementation ${identity.id}`);
   }
   return route;
};

const verifyShippingManifests = async () => {
   const shippingRoot = process.env.OXIDE_COMPARISON_SHIPPING_ROOT;
   if (!shippingRoot) {
      throw new Error("OXIDE_COMPARISON_SHIPPING_ROOT is required when the campaign executes");
   }
   for (const identity of [plan.reference, plan.contender]) {
      const implementation = implementationRoute(identity);
      const manifestPath = path.join(shippingRoot, `${implementation}.routes.v2.json`);
      const encoded = await readFile(manifestPath);
      const sha256 = createHash("sha256").update(encoded).digest("hex");
      expect(sha256, `${implementation} shipping manifest must match the frozen plan`).toBe(identity.shipping_payload_manifest_sha256);
      const manifest = JSON.parse(encoded.toString("utf8")) as ShippingManifest;
      expect(manifest.schema_version).toBe(2);
      expect(manifest.implementation_id).toBe(implementation);
      verifiedShippingManifests.push({
         implementation_id: implementation,
         manifest_path: manifestPath,
         manifest_sha256: sha256,
         totals: manifest.totals,
         category_totals: manifest.category_totals,
         route_class_totals: manifest.route_class_totals,
      });
   }
};

const writeCheckpoint = async (status: string, active: unknown) => {
   const output = {
      schema_version: 1,
      status,
      plan_id: plan.plan_id,
      plan_sha256: plan.plan_sha256,
      harness_sha256: harnessSha256,
      pass_instrumentation_sha256: passInstrumentationSha256,
      web_vitals_runtime_sha256: webVitalsSha256,
      shipping_manifests: verifiedShippingManifests,
      eligibility,
      active,
      completed_pairs: completedPairs,
   };
   const temporary = `${resultPath}.tmp`;
   await mkdir(path.dirname(resultPath), {recursive: true});
   await writeFile(temporary, `${JSON.stringify(output, null, 2)}\n`);
   await rename(temporary, resultPath);
};

const captureNetwork = async (context: BrowserContext, page: Page) => {
   const session = await context.newCDPSession(page);
   const rows = new Map<string, NetworkRow>();
   session.on("Network.responseReceived", event => {
      rows.set(event.requestId, {
         request_id: event.requestId,
         url: event.response.url,
         mime_type: event.response.mimeType || null,
         status: event.response.status,
         protocol: event.response.protocol || null,
         encoded_data_length: null,
         from_disk_cache: event.response.fromDiskCache ?? null,
         from_prefetch_cache: event.response.fromPrefetchCache ?? null,
         from_service_worker: event.response.fromServiceWorker ?? null,
      });
   });
   session.on("Network.loadingFinished", event => {
      const row = rows.get(event.requestId);
      if (row) {
         row.encoded_data_length = event.encodedDataLength;
      }
   });
   await session.send("Network.enable", {maxTotalBufferSize: 100_000_000, maxResourceBufferSize: 10_000_000});
   await session.send("Accessibility.enable");
   await session.send("Performance.enable");
   return {session, rows};
};

const startPresentationTrace = async (session: CDPSession, tracePath: string) => {
   const includedCategories = [
      "benchmark",
      "blink.user_timing",
      "cc",
      "input",
      "latencyInfo",
      "viz",
      "disabled-by-default-devtools.timeline.frame",
   ];
   const supported = await session.send("Tracing.getCategories") as {categories: string[]};
   for (const category of includedCategories) {
      expect(supported.categories, `Chrome presentation trace category ${category} must be available`).toContain(category);
   }
   let maximumBufferUsage = 0;
   let maximumEventCount = 0;
   const recordUsage = (event: {percentFull?: number; value?: number; eventCount?: number}) => {
      maximumBufferUsage = Math.max(maximumBufferUsage, event.percentFull ?? event.value ?? 0);
      maximumEventCount = Math.max(maximumEventCount, event.eventCount ?? 0);
   };
   session.on("Tracing.bufferUsage", recordUsage);
   await session.send("Tracing.start", {
      bufferUsageReportingInterval: 250,
      transferMode: "ReturnAsStream",
      streamFormat: "json",
      streamCompression: "gzip",
      tracingBackend: "chrome",
      traceConfig: {
         recordMode: "recordUntilFull",
         traceBufferSizeInKb: 65_536,
         enableSampling: false,
         enableSystrace: false,
         includedCategories,
      },
   });
   let stopped = false;
   return {
      stop: async () => {
         if (stopped) {
            return null;
         }
         stopped = true;
         const completed = new Promise<TraceCompletion>(resolve => session.once("Tracing.tracingComplete", resolve));
         await session.send("Tracing.end");
         const completion = await completed;
         session.off("Tracing.bufferUsage", recordUsage);
         if (!completion.stream) {
            throw new Error("Chrome presentation trace completed without a stream");
         }
         const chunks: Buffer[] = [];
         try {
            for (;;) {
               const chunk = await session.send("IO.read", {handle: completion.stream}) as {base64Encoded?: boolean; data: string; eof?: boolean};
               chunks.push(Buffer.from(chunk.data, chunk.base64Encoded ? "base64" : "utf8"));
               if (chunk.eof) {
                  break;
               }
            }
         } finally {
            await session.send("IO.close", {handle: completion.stream}).catch(() => undefined);
         }
         const encoded = Buffer.concat(chunks);
         const decoded = gunzipSync(encoded);
         const trace = JSON.parse(decoded.toString("utf8")) as {traceEvents?: Array<{name?: string}>};
         if (!Array.isArray(trace.traceEvents)) {
            throw new Error("Chrome presentation trace has no traceEvents array");
         }
         const requiredFlowEvents = [
            "EventLatency",
            "PipelineReporter",
            "SubmitCompositorFrameToPresentationCompositorFrame",
            "oxide-comparison-trusted-action-begin",
            "oxide-comparison-trusted-action-end",
            "oxide-comparison-visual-generation",
         ];
         const requiredEventCounts = Object.fromEntries(requiredFlowEvents.map(name => [name, 0])) as Record<string, number>;
         for (const event of trace.traceEvents) {
            if (event.name && event.name in requiredEventCounts) {
               requiredEventCounts[event.name] += 1;
            }
         }
         for (const name of requiredFlowEvents) {
            expect(requiredEventCounts[name], `Chrome presentation trace must contain ${name}`).toBeGreaterThan(0);
         }
         await mkdir(path.dirname(tracePath), {recursive: true});
         await writeFile(tracePath, encoded);
         const evidence = {
            schema_version: 1,
            classification: "raw-minimal-presentation-trace-calibration-pending",
            trace_path: path.relative(harnessRoot, tracePath),
            trace_sha256: createHash("sha256").update(encoded).digest("hex"),
            encoded_bytes: encoded.length,
            format: completion.traceFormat ?? "json",
            compression: completion.streamCompression ?? "gzip",
            data_loss_occurred: completion.dataLossOccurred,
            maximum_buffer_usage: maximumBufferUsage,
            maximum_event_count: maximumEventCount,
            included_categories: includedCategories,
            required_flow_event_counts: requiredEventCounts,
            metric_status: "unavailable-until-generation-correlation-and-overhead-calibration-pass",
         };
         expect(evidence.data_loss_occurred, "Chrome presentation trace must not lose data").toBe(false);
         expect(evidence.maximum_buffer_usage, "Chrome presentation trace buffer must not fill").toBeLessThan(1);
         return evidence;
      },
   };
};

const useServerRenderedInitialRoute = async (page: Page, implementation: string, scenarioId: string) => {
   if (implementation !== "html-first") {
      return false;
   }
   const serverScenario = await page.locator("main[data-server-rendered='true']").getAttribute("data-scenario-id");
   expect(serverScenario, "HTML-first initial navigation must already contain the requested server-rendered route").toBe(scenarioId);
   return true;
};

const pngHasChunk = (bytes: Buffer, expected: string) => {
   if (bytes.length < 8 || bytes.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") {
      throw new Error("exact-static input is not a PNG");
   }
   for (let offset = 8; offset + 12 <= bytes.length;) {
      const length = bytes.readUInt32BE(offset);
      const end = offset + 12 + length;
      if (end > bytes.length) {
         throw new Error("exact-static PNG chunk exceeds the encoded input");
      }
      if (bytes.subarray(offset + 4, offset + 8).toString("ascii") === expected) {
         return true;
      }
      offset = end;
   }
   return false;
};

const decodeExactStatic = async (filePath: string, label: string) => {
   const encoded = await readFile(filePath);
   if (!pngHasChunk(encoded, "sRGB")) {
      throw new Error(`${label} PNG does not declare sRGB`);
   }
   const image = PNG.sync.read(encoded, {skipRescale: true});
   if (image.width !== 1170 || image.height !== 2532) {
      throw new Error(`${label} PNG is ${image.width}x${image.height}, expected 1170x2532`);
   }
   for (let offset = 3; offset < image.data.length; offset += 4) {
      if (image.data[offset] !== 255) {
         throw new Error(`${label} PNG contains a nonopaque pixel at index ${(offset - 3) / 4}`);
      }
   }
   return {encoded, image};
};

const compareExactStatic = async (contenderPath: string, referencePath: string, layoutPath: string) => {
   const contender = await decodeExactStatic(contenderPath, "contender");
   const reference = await decodeExactStatic(referencePath, "reference");
   const layout = await readFile(layoutPath);
   const layoutContract = JSON.parse(layout.toString("utf8")) as {coordinate_space: string; root: {x: number; y: number; width: number; height: number}};
   if (layoutContract.coordinate_space !== "logical-points" || layoutContract.root.x !== 0 || layoutContract.root.y !== 0 || layoutContract.root.width !== 390 || layoutContract.root.height !== 844) {
      throw new Error(`exact-static layout ${layoutPath} does not describe the canonical 390x844 logical surface`);
   }
   let differingPixelCount = 0;
   let differingChannelCount = 0;
   let maximumChannelDelta = 0;
   let minimumX = 1170;
   let minimumY = 2532;
   let maximumX = 0;
   let maximumY = 0;
   const channelDeltaHistogram: Record<string, number> = {};
   for (let pixel = 0; pixel < 1170 * 2532; pixel += 1) {
      let differs = false;
      for (let channel = 0; channel < 3; channel += 1) {
         const offset = pixel * 4 + channel;
         const delta = Math.abs(contender.image.data[offset] - reference.image.data[offset]);
         if (delta !== 0) {
            differs = true;
            differingChannelCount += 1;
            maximumChannelDelta = Math.max(maximumChannelDelta, delta);
            channelDeltaHistogram[String(delta)] = (channelDeltaHistogram[String(delta)] ?? 0) + 1;
         }
      }
      if (differs) {
         differingPixelCount += 1;
         const x = pixel % 1170;
         const y = Math.floor(pixel / 1170);
         minimumX = Math.min(minimumX, x);
         minimumY = Math.min(minimumY, y);
         maximumX = Math.max(maximumX, x);
         maximumY = Math.max(maximumY, y);
      }
   }
   return {
      schema_version: 1,
      algorithm: "normalized-srgb8-exact-static-v1",
      contender_png_sha256: createHash("sha256").update(contender.encoded).digest("hex"),
      reference_png_sha256: createHash("sha256").update(reference.encoded).digest("hex"),
      layout_json_sha256: createHash("sha256").update(layout).digest("hex"),
      width: 1170,
      height: 2532,
      canonical_scale: 3,
      compared_pixel_count: 1170 * 2532,
      differing_pixel_count: differingPixelCount,
      differing_channel_count: differingChannelCount,
      maximum_channel_delta: maximumChannelDelta,
      differing_bounds: differingPixelCount === 0 ? null : {x: minimumX, y: minimumY, width: maximumX - minimumX + 1, height: maximumY - minimumY + 1},
      channel_delta_histogram: channelDeltaHistogram,
      accepted: differingPixelCount === 0,
   };
};

const center = async (page: Page, selector: string) => {
   const bounds = await page.locator(selector).boundingBox();
   expect(bounds, `trusted action target ${selector} must be visible`).toBeTruthy();
   return {x: bounds!.x + bounds!.width / 2, y: bounds!.y + bounds!.height / 2};
};

const trustedPrimaryAction = async (page: Page, implementation: string, scenarioId: string) => {
   const oxide = implementation === "oxide";
   if (scenarioId === "startup.first-screen") {
      const point = oxide ? {x: 195, y: 800} : await center(page, ".primary-control");
      await page.mouse.move(point.x, point.y);
      await page.mouse.down();
      await page.mouse.up();
      return "startup.fresh-install-ready";
   }
   if (scenarioId === "dashboard.mixed-static") {
      const point = oxide ? {x: 170, y: 67} : await center(page, "button[data-index='3']");
      await page.mouse.move(point.x, point.y);
      await page.mouse.down();
      await page.mouse.up();
      return "dashboard.leaf-003";
   }
   if (scenarioId === "feed.variable-scroll") {
      const start = oxide ? {x: 195, y: 700} : await center(page, ".feed-list");
      const end = {x: start.x, y: 300};
      await page.mouse.move(start.x, start.y);
      await page.mouse.down();
      await page.mouse.move(end.x, end.y, {steps: 8});
      await page.mouse.up();
      return "feed.forward-drag";
   }
   if (scenarioId === "chat.live-update") {
      const point = oxide ? {x: 195, y: 804} : await center(page, ".chat-composer");
      await page.mouse.move(point.x, point.y);
      await page.mouse.down();
      await page.mouse.up();
      await page.keyboard.type("x");
      return "chat.commit-x";
   }
   if (scenarioId === "navigation.modal") {
      const point = oxide ? {x: 195, y: 488} : await center(page, ".navigation-row[data-index='5']");
      await page.mouse.move(point.x, point.y);
      await page.mouse.down();
      await page.mouse.up();
      return "navigation.item-05";
   }
   if (scenarioId === "image.decode-zoom") {
      const origin = oxide ? {x: 283, y: 422} : await center(page, ".image-stage");
      await page.mouse.move(origin.x, origin.y);
      await page.mouse.down();
      await page.mouse.move(origin.x - 44, origin.y, {steps: 8});
      await page.mouse.up();
      return "image.pan-left";
   }
   throw new Error(`no trusted primary action for ${scenarioId}`);
};

const runSide = async (testOutputRoot: string, chunkId: string, pairIndex: DecimalU64, side: string, identity: ImplementationIdentity, packIds: string[]) => {
   const implementation = implementationRoute(identity);
   const profileRoot = path.join(testOutputRoot, "profiles", implementation, chunkId, pairIndex, side);
   await mkdir(profileRoot, {recursive: true});
   const context = await chromium.launchPersistentContext(profileRoot, {
      channel: "chrome",
      headless: false,
      viewport: {width: 390, height: 844},
      deviceScaleFactor: 3,
   });
   const browserVersion = context.browser()?.version() ?? "unknown";
   expect(browserVersion).toBe(requiredChromeVersion);
   await context.addInitScript({content: commonInitSource});
   const page = context.pages()[0] ?? await context.newPage();
   const network = await captureNetwork(context, page);
   const scenarios: unknown[] = [];
   const tracePath = path.join(testOutputRoot, "traces", implementation, chunkId, pairIndex, `${side}.presentation.json.gz`);
   let presentationTrace: Awaited<ReturnType<typeof startPresentationTrace>> | null = null;
   let presentationTraceEvidence: Awaited<ReturnType<Awaited<ReturnType<typeof startPresentationTrace>>["stop"]>> = null;

   try {
      presentationTrace = await startPresentationTrace(network.session, tracePath);
      for (const packId of packIds) {
         const pack = plan.scenario_packs.find(value => value.id === packId);
         expect(pack, `unknown pack ${packId}`).toBeTruthy();
         for (const scenarioId of pack!.ordered_scenario_ids) {
            expect(plan.scenario_ids).toContain(scenarioId);
            const cell = plan.comparison_cells.find(value => value.pack_id === packId && value.scenario_id === scenarioId);
            expect(cell, `missing comparison cell ${packId}/${scenarioId}`).toBeTruthy();
            await page.goto(`${baseUrl}/${implementation}/index.html?scenario=${encodeURIComponent(scenarioId)}`, {waitUntil: "load"});
            await page.waitForFunction(() => !!window.oxideComparisonV1 && !!window.oxideComparisonObserverV1);
            const preservedServerRoute = await useServerRenderedInitialRoute(page, implementation, scenarioId);
            if (!preservedServerRoute) {
               await page.evaluate(async ({scenarioId, seed}) => {
                  await window.oxideComparisonV1.reset(scenarioId, seed);
                  await window.oxideComparisonV1.ready();
               }, {scenarioId, seed: plan.seed});
            } else {
               await page.evaluate(() => window.oxideComparisonV1.ready());
            }

            const screenshotPath = path.join(testOutputRoot, "screenshots", implementation, chunkId, pairIndex, side, `${scenarioId}.initial.png`);
            await mkdir(path.dirname(screenshotPath), {recursive: true});
            await page.screenshot({path: screenshotPath, clip: {x: 0, y: 0, width: 390, height: 844}, animations: "disabled"});
            const beforeAction = await page.evaluate(() => window.oxideComparisonV1.snapshot()) as {state_hash: string};
            const actionId = `${chunkId}:${pairIndex}:${side}:${packId}:${scenarioId}:trusted-primary`;
            await page.evaluate(actionId => window.oxideComparisonObserverV1.beginTrustedAction(actionId), actionId);
            const trustedActionId = await trustedPrimaryAction(page, implementation, scenarioId);
            await page.evaluate(actionId => window.oxideComparisonObserverV1.endTrustedAction(actionId), actionId);
            await page.evaluate(() => window.oxideComparisonV1.ready());
            await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
            const result = await page.evaluate(async scenarioId => {
               window.oxideComparisonObserverV1.flush("pre-teardown");
               const snapshot = await window.oxideComparisonV1.snapshot();
               return {
                  scenario_id: scenarioId,
                  snapshot,
                  checkpoint: await window.oxideComparisonV1.checkpoint("campaign-settled"),
                  observer: window.oxideComparisonObserverV1.snapshot(),
                  navigation: performance.getEntriesByType("navigation").map(entry => entry.toJSON()),
                  resources: performance.getEntriesByType("resource").map(entry => entry.toJSON()),
               };
            }, scenarioId);
            const rawBrowserAccessibility = await network.session.send("Accessibility.getFullAXTree");
            const jsHeap = await network.session.send("Runtime.getHeapUsage");
            const domCounters = await network.session.send("Memory.getDOMCounters");
            const performanceMetrics = await network.session.send("Performance.getMetrics");
            const browserProcesses = await network.session.send("SystemInfo.getProcessInfo").catch(error => ({unavailable_reason: String(error)}));
            const settledScreenshotPath = path.join(testOutputRoot, "screenshots", implementation, chunkId, pairIndex, side, `${scenarioId}.trusted-settled.png`);
            await page.screenshot({path: settledScreenshotPath, clip: {x: 0, y: 0, width: 390, height: 844}, animations: "disabled"});
            if (implementation !== "oxide") {
               expect((result.snapshot as {gpu_pass_timestamps: unknown}).gpu_pass_timestamps).toBeNull();
            }
            expect((result.snapshot as {state_hash: string}).state_hash, `${scenarioId} trusted action must mutate semantic state`).not.toBe(beforeAction.state_hash);
            expect((result.observer as {invalid_reasons: string[]}).invalid_reasons).toEqual([]);
            scenarios.push({
               pack_id: packId,
               cache_class: cell!.cache_class,
               performance_eligible: false,
               action_classification: "trusted-symmetric-primary-v1",
               trusted_action_id: trustedActionId,
               initial_screenshot_path: path.relative(harnessRoot, screenshotPath),
               initial_screenshot_contract: "normalized-srgb8-exact-static-full-frame",
               settled_screenshot_path: path.relative(harnessRoot, settledScreenshotPath),
               settled_screenshot_contract: "normalized-srgb8-exact-static-full-frame",
               raw_browser_accessibility: rawBrowserAccessibility,
               browser_memory: {js_heap: jsHeap, dom_counters: domCounters},
               browser_performance_metrics: performanceMetrics,
               browser_processes: browserProcesses,
               server_rendered_initial_route_preserved: preservedServerRoute,
               ...result,
            });
            await page.evaluate(() => window.oxideComparisonV1.teardown());
         }
      }
      presentationTraceEvidence = await presentationTrace.stop();
      await writeCheckpoint("side-complete", {chunk_id: chunkId, pair_index: pairIndex, side, implementation_id: identity.id, scenarios, presentation_trace: presentationTraceEvidence});
      return {
         side,
         implementation_id: identity.id,
         implementation_variant: identity.variant,
         browser_version: browserVersion,
         profile_root: path.relative(harnessRoot, profileRoot),
         scenarios,
         network: [...network.rows.values()],
         presentation_trace: presentationTraceEvidence,
      };
   } finally {
      if (presentationTrace && !presentationTraceEvidence) {
         await presentationTrace.stop().catch(() => undefined);
      }
      await network.session.detach().catch(() => undefined);
      await context.close();
   }
};

test("manifest-driven comparison campaign", async ({}, testInfo) => {
   await verifyShippingManifests();
   for (const chunk of plan.controller_chunks) {
      for (const pairIndex of chunk.ordered_pair_indices) {
         const even = BigInt(pairIndex) % 2n === 0n;
         const order = even ? [plan.reference, plan.contender] : [plan.contender, plan.reference];
         const sides = [];
         for (let index = 0; index < order.length; index += 1) {
            sides.push(await runSide(testInfo.outputPath("campaign"), chunk.id, pairIndex, index === 0 ? "a" : "b", order[index], chunk.pack_ids));
         }
         const leftScenarios = sides[0].scenarios as Array<{scenario_id: string; trusted_action_id: string; initial_screenshot_path: string; settled_screenshot_path: string; snapshot: {scene_hash: string; state_hash: string}; checkpoint: {state_hash: string; accessibility_hash: string}}>;
         const rightScenarios = sides[1].scenarios as typeof leftScenarios;
         expect(rightScenarios.length).toBe(leftScenarios.length);
         const staticVisuals = [];
         for (let scenarioIndex = 0; scenarioIndex < leftScenarios.length; scenarioIndex += 1) {
            const left = leftScenarios[scenarioIndex];
            const right = rightScenarios[scenarioIndex];
            expect(right.scenario_id).toBe(left.scenario_id);
            expect(right.trusted_action_id).toBe(left.trusted_action_id);
            expect(right.snapshot.scene_hash, `${left.scenario_id} scene identity must match`).toBe(left.snapshot.scene_hash);
            expect(right.snapshot.state_hash, `${left.scenario_id} settled semantic state must match`).toBe(left.snapshot.state_hash);
            expect(right.checkpoint.state_hash, `${left.scenario_id} checkpoint state must match`).toBe(left.checkpoint.state_hash);
            expect(right.checkpoint.accessibility_hash, `${left.scenario_id} normalized accessibility must match`).toBe(left.checkpoint.accessibility_hash);
            const oxideScenario = sides[0].implementation_id === "oxide.production" ? left : right;
            const referenceScenario = sides[0].implementation_id === "oxide.production" ? right : left;
            const layoutPath = path.resolve(harnessRoot, "../../benchmarks/comparative/specs/v1/layout", `${left.scenario_id}.json`);
            for (const [checkpointId, oxidePath, referencePath] of [
               ["initial", oxideScenario.initial_screenshot_path, referenceScenario.initial_screenshot_path],
               ["trusted-settled", oxideScenario.settled_screenshot_path, referenceScenario.settled_screenshot_path],
            ]) {
               const visual = await compareExactStatic(path.resolve(harnessRoot, oxidePath), path.resolve(harnessRoot, referencePath), layoutPath);
               const visualPath = testInfo.outputPath("campaign", "visual", chunk.id, pairIndex, `${left.scenario_id}.${checkpointId}.exact.json`);
               await mkdir(path.dirname(visualPath), {recursive: true});
               await writeFile(visualPath, `${JSON.stringify(visual, null, 2)}\n`);
               staticVisuals.push({scenario_id: left.scenario_id, checkpoint_id: checkpointId, report_path: path.relative(harnessRoot, visualPath), ...visual});
            }
         }
         const pair = {
            chunk_id: chunk.id,
            pass_id: chunk.pass_id,
            pair_index: pairIndex,
            order: even ? "ab" : "ba",
            checkpoint_generation: chunk.checkpoint_generation,
            semantic_parity: "exact-state-scene-accessibility-hashes",
            static_visuals: staticVisuals,
            static_visual_acceptance_count: staticVisuals.filter(value => value.accepted).length,
            pixel_and_semantic_eligible: staticVisuals.every(value => value.accepted),
            sides,
         };
         completedPairs.push(pair);
         await writeCheckpoint("pair-complete", pair);
      }
   }
   expect(completedPairs.length).toBeGreaterThan(0);
   await writeCheckpoint("complete-correctness-only", null);
});

declare global {
   interface Window {
      oxideComparisonV1: {
         capabilities(): unknown;
         reset(scenarioId: string, seed: string): Promise<number>;
         ready(): Promise<void>;
         snapshot(): Promise<unknown>;
         checkpoint(checkpointId: string): Promise<unknown>;
         teardown(): Promise<number>;
      };
      oxideComparisonObserverV1: {
         beginTrustedAction(actionId: string): void;
         endTrustedAction(actionId: string): void;
         flush(reason: string): void;
         snapshot(): unknown;
      };
   }
}
