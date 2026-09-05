(() => {
   const WEB_VITALS_VERSION = "5.3.0";
   const WEB_VITALS_NPM_INTEGRITY = "sha512-q6LWsLatGYZp5VGBIOvbTj6JBV2nOmC8KvWztXBmwJcfFAzhwKwbOxhUH306XY3CcaZDUlSmSuNPBsCn0bFu+g==";
   const WEB_VITALS_RUNTIME_SHA256 = "d4227a2ad276d3c60e10e80abf54f71676c322558ac3dda42d4b4f71c73d8f83";
   const entries = [];
   const inpUpdates = [];
   const invalidReasons = [];
   const actions = new Map();
   const interactionOwners = new Map();
   const visualGenerations = [];
   const observers = [];
   const resourceTimingBufferSize = 10_000;
   let activeActionId = null;
   let droppedEntryCount = 0;
   let resourceBufferFull = false;
   let visibilityFlushCount = 0;

   const markVisualGeneration = target => {
      const generation = target.getAttribute?.("data-visual-generation");
      if (generation === null || generation === undefined) {
         return;
      }
      const detail = {
         generation,
         scenario_id: new URL(location.href).searchParams.get("scenario"),
         target: target.id || target.tagName?.toLowerCase() || "unknown",
      };
      const mark = performance.mark("oxide-comparison-visual-generation", {detail});
      visualGenerations.push({start_time_ms: mark.startTime, ...detail});
   };

   const visualGenerationObserver = new MutationObserver(records => {
      for (const record of records) {
         markVisualGeneration(record.target);
      }
   });
   visualGenerationObserver.observe(document, {subtree: true, attributes: true, attributeFilter: ["data-visual-generation"]});

   if (!globalThis.webVitals?.onINP) {
      throw new Error(`web-vitals ${WEB_VITALS_VERSION} must be installed before observer.init.js`);
   }

   const serialize = value => {
      if (value && typeof value.toJSON === "function") {
         return value.toJSON();
      }
      return structuredClone(value);
   };

   const actionForEntry = entry => {
      if (activeActionId) {
         return actions.get(activeActionId);
      }
      const candidates = [...actions.values()].filter(action =>
         entry.startTime >= action.started_at_ms &&
         entry.startTime <= (action.closed_at_ms ?? performance.now()) + 1_000
      );
      return candidates.at(-1) ?? null;
   };

   const attributeInteraction = entry => {
      if (!entry.interactionId) {
         return;
      }
      const action = actionForEntry(entry);
      if (!action) {
         invalidReasons.push(`unexpected interactionId ${entry.interactionId}`);
         return;
      }
      const owner = interactionOwners.get(entry.interactionId);
      if (owner && owner !== action.id) {
         invalidReasons.push(`interactionId ${entry.interactionId} belongs to both ${owner} and ${action.id}`);
         return;
      }
      interactionOwners.set(entry.interactionId, action.id);
      action.interaction_ids.add(entry.interactionId);
   };

   const recordEntries = rawEntries => {
      for (const entry of rawEntries) {
         if (entry.entryType === "event" || entry.entryType === "first-input") {
            attributeInteraction(entry);
         }
         entries.push(serialize(entry));
      }
   };

   performance.setResourceTimingBufferSize(resourceTimingBufferSize);
   addEventListener("resourcetimingbufferfull", () => {
      resourceBufferFull = true;
      invalidReasons.push("resource timing buffer filled");
   });

   const entryTypes = ["navigation", "resource", "paint", "event", "first-input", "longtask", "long-animation-frame", "layout-shift"];
   for (const type of entryTypes) {
      if (!PerformanceObserver.supportedEntryTypes?.includes(type)) {
         continue;
      }
      try {
         const observer = new PerformanceObserver((list, _observer, options) => {
            recordEntries(list.getEntries());
            if (Number.isFinite(options?.droppedEntriesCount) && options.droppedEntriesCount > 0) {
               droppedEntryCount += options.droppedEntriesCount;
               invalidReasons.push(`${type} observer dropped ${options.droppedEntriesCount} entries`);
            }
         });
         observer.observe(type === "event" ? {type, buffered: true, durationThreshold: 16} : {type, buffered: true});
         observers.push({observer, type});
      } catch (error) {
         invalidReasons.push(`${type} observer failed: ${String(error)}`);
      }
   }

   webVitals.onINP(metric => {
      for (const entry of metric.entries) {
         attributeInteraction(entry);
      }
      inpUpdates.push({
         name: metric.name,
         value: metric.value,
         rating: metric.rating,
         delta: metric.delta,
         id: metric.id,
         navigation_type: metric.navigationType,
         entries: metric.entries.map(serialize),
      });
   }, {reportAllChanges: true, durationThreshold: 16});

   const flush = reason => {
      for (const {observer} of observers) {
         recordEntries(observer.takeRecords());
      }
      entries.push({entryType: "observer-flush", name: reason, startTime: performance.now(), duration: 0});
   };

   addEventListener("visibilitychange", () => {
      if (document.visibilityState === "hidden") {
         visibilityFlushCount += 1;
         flush("visibility-hidden");
      }
   }, {capture: true});

   const api = Object.freeze({
      schema_version: 1,
      web_vitals_package: "web-vitals",
      web_vitals_version: WEB_VITALS_VERSION,
      web_vitals_npm_integrity: WEB_VITALS_NPM_INTEGRITY,
      web_vitals_runtime_sha256: WEB_VITALS_RUNTIME_SHA256,
      beginTrustedAction(actionId) {
         if (!actionId || actions.has(actionId) || activeActionId) {
            throw new Error(`trusted action cannot begin: ${actionId}`);
         }
         activeActionId = actionId;
         actions.set(actionId, {id: actionId, started_at_ms: performance.now(), closed_at_ms: null, interaction_ids: new Set()});
         performance.mark("oxide-comparison-trusted-action-begin", {detail: {action_id: actionId}});
      },
      endTrustedAction(actionId) {
         if (activeActionId !== actionId) {
            throw new Error(`trusted action is not active: ${actionId}`);
         }
         actions.get(actionId).closed_at_ms = performance.now();
         performance.mark("oxide-comparison-trusted-action-end", {detail: {action_id: actionId}});
         activeActionId = null;
      },
      flush,
      snapshot() {
         flush("snapshot");
         return {
            schema_version: 1,
            web_vitals: {
               package: "web-vitals",
               version: WEB_VITALS_VERSION,
               npm_integrity: WEB_VITALS_NPM_INTEGRITY,
               runtime_sha256: WEB_VITALS_RUNTIME_SHA256,
            },
            entries: structuredClone(entries),
            inp_updates: structuredClone(inpUpdates),
            trusted_actions: [...actions.values()].map(action => ({
               id: action.id,
               started_at_ms: action.started_at_ms,
               closed_at_ms: action.closed_at_ms,
               interaction_ids: [...action.interaction_ids],
               event_timing_status: action.interaction_ids.size ? "observed" : "below_threshold_or_unavailable",
            })),
            interaction_owners: Object.fromEntries(interactionOwners),
            visual_generations: structuredClone(visualGenerations),
            resource_timing_buffer_size: resourceTimingBufferSize,
            resource_buffer_full: resourceBufferFull,
            dropped_entry_count: droppedEntryCount,
            visibility_flush_count: visibilityFlushCount,
            invalid_reasons: [...new Set(invalidReasons)],
         };
      },
   });
   Object.defineProperty(window, "oxideComparisonObserverV1", {value: api, configurable: false, writable: false});
})();
