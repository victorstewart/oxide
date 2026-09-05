const STATE_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.state");
const SNAPSHOT_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.snapshot.v1");
const SHUTDOWN_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.shutdown.v1");
const PROTOCOL_VERSION = 3;

function snapshot(state)
{
   return Object.freeze({
      protocol_version: PROTOCOL_VERSION,
      generation: 0,
      device_request_count: 0,
      live_device_count: 0,
      renderer_lease_count: state.rendererLeaseCount,
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
   }
   return snapshot(state);
}

function createState()
{
   const state = {
      protocolVersion: PROTOCOL_VERSION,
      rendererLeaseCount: 0,
      sessionShutdownCount: 0,
      closed: false,
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
   const lease = { released: false };
   state.rendererLeaseCount += 1;
   return lease;
}

export function releaseOxideWebGpuDeviceSession(lease)
{
   if (!lease || lease.released) {
      return;
   }
   lease.released = true;
   MODULE_STATE.rendererLeaseCount = Math.max(0, MODULE_STATE.rendererLeaseCount - 1);
}
