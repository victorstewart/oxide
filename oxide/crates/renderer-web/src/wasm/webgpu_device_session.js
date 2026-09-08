const DEVICE_LABEL = "oxide-webgpu-shared-device-v1";
const STATE_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.state");
const SNAPSHOT_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.snapshot.v1");
const SHUTDOWN_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.shutdown.v1");
const MODULE_TOKEN = Object.freeze({});
const PROTOCOL_VERSION = 9;

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
      incompatible_module_failure_count: state.incompatibleModuleFailureCount,
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
   state.adapterPromises.clear();
   if (state.currentGeneration) {
      destroyGeneration(state, state.currentGeneration);
      state.currentGeneration = null;
   }
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
      incompatibleModuleFailureCount: 0,
      moduleOwnerToken: null,
      sessionShutdownCount: 0,
      closed: false,
      gpu: null,
      originalRequestAdapter: null,
      patchedRequestAdapter: null,
      adapterPromises: new Map(),
      adapterDescriptorKeys: new WeakMap(),
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
   state.adapterPromises ??= new Map();
   state.adapterDescriptorKeys ??= new WeakMap();
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

function adapterDescriptorKey(descriptor)
{
   const options = descriptor ?? {};
   const extraOptions = Object.entries(options)
      .filter(([name, value]) => value !== undefined
         && name !== "powerPreference"
         && name !== "forceFallbackAdapter"
         && name !== "xrCompatible")
      .map(([name, value]) => [name, typeof value, String(value)])
      .sort(([left], [right]) => left.localeCompare(right));
   return JSON.stringify([
      String(options.powerPreference ?? ""),
      Boolean(options.forceFallbackAdapter ?? false),
      Boolean(options.xrCompatible ?? false),
      extraOptions,
   ]);
}

function createGeneration(state)
{
   const generation = {
      id: state.nextGeneration,
      adapterDescriptorKey: null,
      descriptorKey: null,
      devicePromise: null,
      device: null,
      live: false,
      destroyed: false,
      destroyWhenReady: false,
   };
   state.nextGeneration += 1;
   state.currentGeneration = generation;
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
         if (state.currentGeneration === generation) {
            state.currentGeneration = null;
         }
      }, () => {
         markDeviceNotLive(state, generation);
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

function registerAdapter(state, key, adapter)
{
   let keys = state.adapterDescriptorKeys.get(adapter);
   if (!keys) {
      keys = new Set();
      state.adapterDescriptorKeys.set(adapter, keys);
   }
   keys.add(key);
   installAdapterRequestDevicePatch(state, adapter);
   return adapter;
}

function installAdapterRequestDevicePatch(state, adapter)
{
   if (!adapter || typeof adapter.requestDevice !== "function") {
      throw new Error("browser GPUAdapter requestDevice unavailable");
   }
   const installed = state.adapterPatches.get(adapter);
   if (installed && adapter.requestDevice === installed.patched) {
      return;
   }
   if (installed) {
      throw new Error("browser GPUAdapter requestDevice changed after Oxide initialization");
   }

   // Chromium's WebGPU implementation keeps the native Instance associated with
   // the concrete adapter receiver. Retain that receiver in the callable for the
   // full page session instead of retaining only an unbound prototype function.
   const ownerAdapter = adapter;
   const originalRequestDevice = ownerAdapter.requestDevice.bind(ownerAdapter);
   const patchedRequestDevice = function(deviceDescriptor)
   {
      if (!deviceDescriptor || deviceDescriptor.label !== DEVICE_LABEL) {
         return originalRequestDevice(...arguments);
      }
      const generation = state.currentGeneration;
      if (!generation || state.closed) {
         return Promise.reject(new Error("Oxide WebGPU device requested without an active page session"));
      }
      const adapterKeys = state.adapterDescriptorKeys.get(ownerAdapter);
      if (!adapterKeys || adapterKeys.size !== 1) {
         state.incompatibleAcquireFailureCount += 1;
         return Promise.reject(new Error("incompatible Oxide WebGPU adapter requirements"));
      }
      const adapterKey = adapterKeys.values().next().value;
      if (generation.adapterDescriptorKey !== null
         && generation.adapterDescriptorKey !== undefined
         && generation.adapterDescriptorKey !== adapterKey) {
         state.incompatibleAcquireFailureCount += 1;
         return Promise.reject(new Error("incompatible Oxide WebGPU adapter requirements"));
      }
      generation.adapterDescriptorKey = adapterKey;
      const key = descriptorKey(deviceDescriptor);
      if (generation.descriptorKey !== null && generation.descriptorKey !== key) {
         state.incompatibleAcquireFailureCount += 1;
         return Promise.reject(new Error("incompatible Oxide WebGPU device requirements"));
      }
      generation.descriptorKey = key;
      if (!generation.devicePromise) {
         state.deviceRequestCount += 1;
         generation.devicePromise = Promise.resolve()
            .then(() => originalRequestDevice(deviceDescriptor))
            .then(
               (device) => registerDevice(state, generation, device),
               (error) => {
                  generation.devicePromise = null;
                  generation.adapterDescriptorKey = null;
                  generation.descriptorKey = null;
                  throw error;
               },
            );
      }
      return generation.devicePromise;
   };

   Object.defineProperty(adapter, "requestDevice", {
      value: patchedRequestDevice,
      configurable: true,
   });
   state.adapterPatches.set(adapter, {
      original: originalRequestDevice,
      patched: patchedRequestDevice,
   });
}

function installRequestAdapterPatch(state)
{
   const gpu = globalThis.navigator?.gpu;
   if (!gpu || typeof gpu.requestAdapter !== "function") {
      throw new Error("browser GPU requestAdapter unavailable");
   }
   if (state.gpu) {
      if (gpu !== state.gpu || gpu.requestAdapter !== state.patchedRequestAdapter) {
         throw new Error("browser GPU requestAdapter changed after Oxide initialization");
      }
      return;
   }

   // Keep the concrete GPU receiver alive with its native callable. This is
   // required by Chromium across independently instantiated WASM modules.
   const originalRequestAdapter = gpu.requestAdapter.bind(gpu);
   const patchedRequestAdapter = function(adapterDescriptor)
   {
      const key = adapterDescriptorKey(adapterDescriptor);
      if (state.closed) {
         return Promise.resolve(originalRequestAdapter(...arguments))
            .then((adapter) => {
               if (adapter) {
                  installAdapterRequestDevicePatch(state, adapter);
               }
               return adapter;
            });
      }
      const existing = state.adapterPromises.get(key);
      if (existing) {
         return existing;
      }
      const adapterPromise = Promise.resolve(originalRequestAdapter(...arguments))
         .then(
            (adapter) => {
               if (!adapter) {
                  state.adapterPromises.delete(key);
                  return adapter;
               }
               return registerAdapter(state, key, adapter);
            },
            (error) => {
               state.adapterPromises.delete(key);
               throw error;
            },
         );
      state.adapterPromises.set(key, adapterPromise);
      return adapterPromise;
   };
   Object.defineProperty(gpu, "requestAdapter", {
      value: patchedRequestAdapter,
      configurable: true,
   });
   state.gpu = gpu;
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
   if (state.moduleOwnerToken === null) {
      state.moduleOwnerToken = MODULE_TOKEN;
   } else if (state.moduleOwnerToken !== MODULE_TOKEN) {
      state.incompatibleModuleFailureCount += 1;
      throw new Error("Oxide WebGPU page session belongs to another compiled WASM module");
   }
   installRequestAdapterPatch(state);
   const generation = state.currentGeneration ?? createGeneration(state);
   state.rendererLeaseCount += 1;
   return { generation, released: false };
}

export function releaseOxideWebGpuDeviceSession(lease)
{
   if (!lease || lease.released) {
      return;
   }
   lease.released = true;
   const state = MODULE_STATE;
   state.rendererLeaseCount = Math.max(0, state.rendererLeaseCount - 1);
}
