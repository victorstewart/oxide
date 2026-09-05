const DEVICE_LABEL = "oxide-webgpu-renderer-device-v2";
const STATE_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.state");
const SNAPSHOT_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.snapshot.v1");
const SHUTDOWN_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.shutdown.v1");
const PROTOCOL_VERSION = 2;

function snapshot(state)
{
   return Object.freeze({
      protocol_version: PROTOCOL_VERSION,
      generation: state.currentGeneration?.id ?? 0,
      device_request_count: state.deviceRequestCount,
      live_device_count: state.liveDeviceCount,
      renderer_lease_count: state.rendererLeaseCount,
      device_destroy_count: state.deviceDestroyCount,
      incompatible_acquire_failure_count: state.incompatibleAcquireFailureCount,
      session_shutdown_count: state.sessionShutdownCount,
      closed: state.closed,
   });
}

function markDeviceNotLive(state, generation)
{
   if (!generation.live) {
      return;
   }
   generation.live = false;
   state.liveDeviceCount -= 1;
}

function destroyGeneration(state, generation)
{
   if (generation.destroyed) {
      return;
   }
   if (!generation.device) {
      generation.destroyWhenReady = true;
      return;
   }
   generation.destroyed = true;
   markDeviceNotLive(state, generation);
   generation.device.destroy();
   state.deviceDestroyCount += 1;
}

function shutdown(state)
{
   if (state.closed) {
      return snapshot(state);
   }
   state.closed = true;
   state.sessionShutdownCount += 1;
   for (const generation of state.generations) {
      destroyGeneration(state, generation);
   }
   state.generations.clear();
   state.currentGeneration = null;
   return snapshot(state);
}

function createState()
{
   const state = {
      protocolVersion: PROTOCOL_VERSION,
      currentGeneration: null,
      nextGeneration: 1,
      rendererLeaseCount: 0,
      deviceRequestCount: 0,
      liveDeviceCount: 0,
      deviceDestroyCount: 0,
      incompatibleAcquireFailureCount: 0,
      sessionShutdownCount: 0,
      closed: false,
      generations: new Set(),
      pendingLeases: [],
      adapterLeases: new WeakMap(),
      gpuPrototype: null,
      originalRequestAdapter: null,
      patchedRequestAdapter: null,
      adapterPatches: new WeakMap(),
   };
   Object.defineProperty(globalThis, STATE_SYMBOL, {
      value: state,
      configurable: false,
      enumerable: false,
      writable: false,
   });
   Object.defineProperty(globalThis, SNAPSHOT_SYMBOL, {
      value: () => snapshot(state),
      configurable: false,
      enumerable: false,
      writable: false,
   });
   Object.defineProperty(globalThis, SHUTDOWN_SYMBOL, {
      value: () => shutdown(state),
      configurable: false,
      enumerable: false,
      writable: false,
   });
   if (typeof globalThis.addEventListener === "function") {
      globalThis.addEventListener("pagehide", (event) => {
         if (!event.persisted) {
            shutdown(state);
         }
      }, { capture: true });
   }
   return state;
}

function sharedState()
{
   const state = globalThis[STATE_SYMBOL] ?? createState();
   if (state.protocolVersion !== PROTOCOL_VERSION) {
      throw new Error("incompatible Oxide WebGPU device-session protocol");
   }
   return state;
}

function descriptorKey(descriptor)
{
   const requiredFeatures = Array.from(descriptor.requiredFeatures ?? [], String).sort();
   const requiredLimits = Object.entries(descriptor.requiredLimits ?? {})
      .map(([name, value]) => [name, String(value)])
      .sort(([left], [right]) => left.localeCompare(right));
   const queueLabel = String(descriptor.defaultQueue?.label ?? "");
   return JSON.stringify([requiredFeatures, requiredLimits, queueLabel]);
}

function createGeneration(state, lease)
{
   const generation = {
      id: state.nextGeneration,
      descriptorKey: null,
      devicePromise: null,
      device: null,
      live: false,
      destroyed: false,
      destroyWhenReady: false,
   };
   state.nextGeneration += 1;
   state.generations.add(generation);
   state.currentGeneration = generation;
   lease.generation = generation;
   return generation;
}

function registerDevice(state, generation, device)
{
   generation.device = device;
   generation.live = true;
   state.liveDeviceCount += 1;
   const lost = device.lost;
   if (lost && typeof lost.then === "function") {
      lost.then(() => {
         markDeviceNotLive(state, generation);
         state.generations.delete(generation);
         if (state.currentGeneration === generation) {
            state.currentGeneration = null;
         }
      }, () => {
         markDeviceNotLive(state, generation);
         state.generations.delete(generation);
         if (state.currentGeneration === generation) {
            state.currentGeneration = null;
         }
      });
   }
   if (generation.destroyWhenReady || state.closed) {
      destroyGeneration(state, generation);
   }
   return device;
}

function installAdapterRequestDevicePatch(state, adapter)
{
   const adapterPrototype = Object.getPrototypeOf(adapter);
   if (!adapterPrototype || typeof adapterPrototype.requestDevice !== "function") {
      throw new Error("browser GPUAdapter prototype unavailable");
   }
   const installed = state.adapterPatches.get(adapterPrototype);
   if (installed && adapterPrototype.requestDevice === installed.patched) {
      return;
   }
   if (installed) {
      throw new Error("browser GPUAdapter requestDevice changed after Oxide initialization");
   }

   const descriptor = Object.getOwnPropertyDescriptor(adapterPrototype, "requestDevice");
   const originalRequestDevice = adapterPrototype.requestDevice;
   const patchedRequestDevice = function(deviceDescriptor)
   {
      if (!deviceDescriptor || deviceDescriptor.label !== DEVICE_LABEL) {
         return Reflect.apply(originalRequestDevice, this, arguments);
      }
      const lease = state.adapterLeases.get(this);
      if (!lease || lease.released || lease.generation || state.closed) {
         return Promise.reject(new Error("Oxide WebGPU device requested without an active page session"));
      }
      const generation = createGeneration(state, lease);
      generation.descriptorKey = descriptorKey(deviceDescriptor);
      state.deviceRequestCount += 1;
      generation.devicePromise = Promise.resolve()
         .then(() => Reflect.apply(originalRequestDevice, this, [deviceDescriptor]))
         .then(
            (device) => registerDevice(state, generation, device),
            (error) => {
               state.generations.delete(generation);
               lease.generation = null;
               generation.devicePromise = null;
               generation.descriptorKey = null;
               throw error;
            },
         );
      return generation.devicePromise;
   };

   Object.defineProperty(adapterPrototype, "requestDevice", {
      value: patchedRequestDevice,
      configurable: descriptor?.configurable ?? true,
      enumerable: descriptor?.enumerable ?? false,
      writable: descriptor?.writable ?? true,
   });
   state.adapterPatches.set(adapterPrototype, {
      original: originalRequestDevice,
      patched: patchedRequestDevice,
   });
}

function installRequestAdapterPatch(state)
{
   const gpu = globalThis.navigator?.gpu;
   const gpuPrototype = gpu && Object.getPrototypeOf(gpu);
   if (!gpuPrototype || typeof gpuPrototype.requestAdapter !== "function") {
      throw new Error("browser GPU requestAdapter unavailable");
   }
   if (state.gpuPrototype) {
      if (gpuPrototype !== state.gpuPrototype
         || gpuPrototype.requestAdapter !== state.patchedRequestAdapter) {
         throw new Error("browser GPU requestAdapter changed after Oxide initialization");
      }
      return;
   }

   const descriptor = Object.getOwnPropertyDescriptor(gpuPrototype, "requestAdapter");
   const originalRequestAdapter = gpuPrototype.requestAdapter;
   const patchedRequestAdapter = function()
   {
      return Promise.resolve(Reflect.apply(originalRequestAdapter, this, arguments))
         .then((adapter) => {
            if (adapter) {
               while (state.pendingLeases[0]?.released) {
                  state.pendingLeases.shift();
               }
               const lease = state.pendingLeases.shift();
               if (lease) {
                  state.adapterLeases.set(adapter, lease);
               }
               installAdapterRequestDevicePatch(state, adapter);
            }
            return adapter;
         });
   };
   Object.defineProperty(gpuPrototype, "requestAdapter", {
      value: patchedRequestAdapter,
      configurable: descriptor?.configurable ?? true,
      enumerable: descriptor?.enumerable ?? false,
      writable: descriptor?.writable ?? true,
   });
   state.gpuPrototype = gpuPrototype;
   state.originalRequestAdapter = originalRequestAdapter;
   state.patchedRequestAdapter = patchedRequestAdapter;
}

const MODULE_STATE = sharedState();

export function acquireOxideWebGpuDeviceSession()
{
   const state = sharedState();
   if (state.closed) {
      throw new Error("Oxide WebGPU page session is shut down");
   }
   installRequestAdapterPatch(state);
   const lease = { generation: null, released: false };
   state.pendingLeases.push(lease);
   state.rendererLeaseCount += 1;
   return lease;
}

export function releaseOxideWebGpuDeviceSession(lease)
{
   if (!lease || lease.released) {
      return;
   }
   lease.released = true;
   const state = MODULE_STATE;
   if (lease.generation) {
      destroyGeneration(state, lease.generation);
      state.generations.delete(lease.generation);
      if (state.currentGeneration === lease.generation) {
         state.currentGeneration = null;
      }
   }
   state.rendererLeaseCount = Math.max(0, state.rendererLeaseCount - 1);
}
