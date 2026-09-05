const API_VERSION = 1;

const stableValue = value => {
   if (Array.isArray(value)) {
      return value.map(stableValue);
   }
   if (value && typeof value === "object") {
      return Object.fromEntries(Object.keys(value).sort().map(key => [key, stableValue(value[key])]));
   }
   return value;
};

const hash = async value => {
   const bytes = new TextEncoder().encode(JSON.stringify(stableValue(value)));
   const digest = await crypto.subtle.digest("SHA-256", bytes);
   return [...new Uint8Array(digest)].map(value => value.toString(16).padStart(2, "0")).join("");
};

export const installComparisonControl = (adapter, initialGeneration = 0) => {
   let generation = initialGeneration;
   let error = null;
   const invoke = async operation => {
      try {
         error = null;
         return await operation();
      } catch (cause) {
         error = String(cause?.stack || cause);
         throw cause;
      }
   };
   const api = Object.freeze({
      capabilities: () => ({
         schema_version: API_VERSION,
         control_api: "oxideComparisonV1",
         ...adapter.capabilities(),
      }),
      reset: (scenarioId, seed) => invoke(async () => {
         generation += 1;
         await adapter.reset(scenarioId, seed, generation);
         return generation;
      }),
      advance: checkpointId => invoke(async () => {
         const startedAt = performance.now();
         await adapter.advance(checkpointId);
         generation += 1;
         const outputReadyMs = performance.now() - startedAt;
         performance.mark("oxide-comparison-output-ready", {detail: {checkpoint_id: checkpointId, generation, output_ready_ms: outputReadyMs}});
         return {
            schema_version: API_VERSION,
            checkpoint_id: checkpointId,
            generation,
            output_ready_ms: outputReadyMs,
         };
      }),
      ready: () => invoke(() => adapter.ready()),
      snapshot: () => invoke(async () => {
         const snapshot = await adapter.snapshot();
         return {
            schema_version: API_VERSION,
            generation,
            scene_hash: await hash(snapshot.scene),
            state_hash: await hash(snapshot.state),
            target_geometry: snapshot.target_geometry,
            logical_counters: snapshot.logical_counters,
            gpu_pass_timestamps: snapshot.gpu_pass_timestamps ?? null,
            error,
         };
      }),
      checkpoint: checkpointId => invoke(async () => {
         const checkpoint = await adapter.checkpoint(checkpointId);
         return {
            schema_version: API_VERSION,
            generation,
            checkpoint_id: checkpointId,
            state_hash: await hash(checkpoint.state),
            accessibility_hash: await hash(checkpoint.accessibility),
            ...checkpoint,
         };
      }),
      teardown: () => invoke(async () => {
         await adapter.teardown();
         generation += 1;
         return generation;
      }),
   });
   Object.defineProperty(window, "oxideComparisonV1", {value: api, configurable: false, writable: false});
   return api;
};
