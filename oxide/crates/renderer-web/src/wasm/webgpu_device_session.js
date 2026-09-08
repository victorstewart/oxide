const STATE_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.state");
const SNAPSHOT_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.snapshot.v1");
const SHUTDOWN_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.shutdown.v1");
const MODULE_TOKEN = Object.freeze({});
const PROTOCOL_VERSION = 10;

function snapshot(state)
{
   return Object.freeze({
      protocol_version: PROTOCOL_VERSION,
      renderer_lease_count: state.rendererLeaseCount,
      pending_renderer_initialization_count: state.pendingRendererInitializationCount,
      incompatible_module_failure_count: state.incompatibleModuleFailureCount,
      session_shutdown_count: state.sessionShutdownCount,
      closed: state.closed,
   });
}

function finishInitialization(state, lease)
{
   if (!lease || lease.initializationFinished) {
      return;
   }
   lease.initializationFinished = true;
   state.pendingRendererInitializationCount = Math.max(
      0,
      state.pendingRendererInitializationCount - 1,
   );
   lease.finish();
}

function shutdown(state)
{
   if (state.closed) {
      return snapshot(state);
   }
   state.closed = true;
   state.sessionShutdownCount += 1;
   for (const lease of state.leases) {
      finishInitialization(state, lease);
   }
   return snapshot(state);
}

function createState()
{
   const state = {
      protocolVersion: PROTOCOL_VERSION,
      moduleOwnerToken: null,
      rendererLeaseCount: 0,
      pendingRendererInitializationCount: 0,
      incompatibleModuleFailureCount: 0,
      sessionShutdownCount: 0,
      closed: false,
      initializationTail: Promise.resolve(),
      leases: new Set(),
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
   const ready = state.initializationTail;
   let finish;
   const finished = new Promise((resolve) => {
      finish = resolve;
   });
   const lease = {
      released: false,
      initializationFinished: false,
      ready,
      finish,
   };
   state.initializationTail = ready.then(() => finished);
   state.leases.add(lease);
   state.rendererLeaseCount += 1;
   state.pendingRendererInitializationCount += 1;
   return lease;
}

export function waitForOxideWebGpuDeviceInitialization(lease)
{
   if (!lease || lease.released) {
      return Promise.reject(new Error("Oxide WebGPU renderer lease is not live"));
   }
   return lease.ready.then(() => {
      if (lease.released || sharedState().closed) {
         throw new Error("Oxide WebGPU renderer cannot initialize after release or page shutdown");
      }
   });
}

export function completeOxideWebGpuDeviceInitialization(lease)
{
   const state = sharedState();
   if (!lease || lease.released) {
      throw new Error("Oxide WebGPU renderer lease is not live");
   }
   finishInitialization(state, lease);
}

export function releaseOxideWebGpuDeviceSession(lease)
{
   if (!lease || lease.released) {
      return;
   }
   finishInitialization(MODULE_STATE, lease);
   lease.released = true;
   MODULE_STATE.leases.delete(lease);
   MODULE_STATE.rendererLeaseCount = Math.max(0, MODULE_STATE.rendererLeaseCount - 1);
}
