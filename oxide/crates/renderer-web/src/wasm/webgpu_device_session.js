const STATE_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.state");
const SNAPSHOT_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.snapshot.v1");
const SHUTDOWN_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.shutdown.v1");
const PROTOCOL_VERSION = 5;

function snapshot(state)
{
   return Object.freeze({
      protocol_version: PROTOCOL_VERSION,
      generation: 0,
      device_request_count: 0,
      live_device_count: 0,
      renderer_lease_count: state.rendererLeaseCount,
      pending_renderer_initialization_count: state.pendingRendererInitializationCount,
      device_destroy_count: 0,
      incompatible_acquire_failure_count: 0,
      session_shutdown_count: state.sessionShutdownCount,
      closed: state.closed,
   });
}

function shutdown(state)
{
   if (!state.closed) {
      state.closed = true;
      state.sessionShutdownCount += 1;
      for (const lease of state.leases) {
         finishInitialization(state, lease);
      }
   }
   return snapshot(state);
}

function createState()
{
   const state = {
      protocolVersion: PROTOCOL_VERSION,
      rendererLeaseCount: 0,
      pendingRendererInitializationCount: 0,
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
   const ready = state.initializationTail;
   let finish;
   const finished = new Promise((resolve) => {
      finish = resolve;
   });
   const lease = {
      released: false,
      initializationFinished: false,
      initializationFinishScheduled: false,
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
         throw new Error("Oxide WebGPU renderer lease cannot initialize after release or page shutdown");
      }
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

function settleThenFinishInitialization(state, lease)
{
   if (!lease || lease.initializationFinished || lease.initializationFinishScheduled) {
      return;
   }
   lease.initializationFinishScheduled = true;
   setTimeout(() => finishInitialization(state, lease), 0);
}

export function completeOxideWebGpuDeviceInitialization(lease)
{
   const state = sharedState();
   if (!lease || lease.released) {
      throw new Error("Oxide WebGPU renderer lease is not live");
   }
   settleThenFinishInitialization(state, lease);
}

export function releaseOxideWebGpuDeviceSession(lease)
{
   if (!lease || lease.released) {
      return;
   }
   settleThenFinishInitialization(MODULE_STATE, lease);
   lease.released = true;
   MODULE_STATE.leases.delete(lease);
   MODULE_STATE.rendererLeaseCount = Math.max(0, MODULE_STATE.rendererLeaseCount - 1);
}
