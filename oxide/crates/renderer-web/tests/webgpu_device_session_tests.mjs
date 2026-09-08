import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const SNAPSHOT_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.snapshot.v1");
const SHUTDOWN_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.shutdown.v1");

test("separate wasm modules retain page lifecycle without intercepting WebGPU ownership", async () => {
   let nativeAdapterRequests = 0;
   let nativeDeviceRequests = 0;
   let nativeDeviceDestroys = 0;
   const events = new EventTarget();
   globalThis.addEventListener = events.addEventListener.bind(events);
   globalThis.dispatchEvent = events.dispatchEvent.bind(events);

   class MockGpuAdapter
   {
      requestDevice()
      {
         nativeDeviceRequests += 1;
         return Promise.resolve({
            destroy()
            {
               nativeDeviceDestroys += 1;
            },
         });
      }
   }
   class MockGpu
   {
      requestAdapter()
      {
         nativeAdapterRequests += 1;
         return Promise.resolve(new MockGpuAdapter());
      }
   }
   Object.defineProperty(globalThis, "navigator", {
      value: { gpu: new MockGpu() },
      configurable: true,
   });

   const sourceUrl = new URL("../src/wasm/webgpu_device_session.js", import.meta.url);
   const source = await readFile(sourceUrl, "utf8");
   const importCopy = (name) => import(
      `data:text/javascript;base64,${Buffer.from(`${source}\n// ${name}`).toString("base64")}`
   );
   const [landingModule, foundationModule] = await Promise.all([
      importCopy("landing wasm module"),
      importCopy("foundation wasm module"),
   ]);
   const landingLease = landingModule.acquireOxideWebGpuDeviceSession();
   const foundationLease = foundationModule.acquireOxideWebGpuDeviceSession();
   const landingReady = landingModule.waitForOxideWebGpuDeviceInitialization(landingLease);
   const foundationReady = foundationModule.waitForOxideWebGpuDeviceInitialization(foundationLease);
   let foundationInitializationStarted = false;
   foundationReady.then(() => {
      foundationInitializationStarted = true;
   });

   await landingReady;
   await Promise.resolve();
   assert.equal(foundationInitializationStarted, false);
   const landingAdapter = await globalThis.navigator.gpu.requestAdapter();
   const landingDevice = await landingAdapter.requestDevice();
   landingModule.completeOxideWebGpuDeviceInitialization(landingLease);
   await foundationReady;
   assert.equal(foundationInitializationStarted, true);
   const foundationAdapter = await globalThis.navigator.gpu.requestAdapter();
   const foundationDevice = await foundationAdapter.requestDevice();
   foundationModule.completeOxideWebGpuDeviceInitialization(foundationLease);
   assert.notStrictEqual(landingAdapter, foundationAdapter);
   assert.notStrictEqual(landingDevice, foundationDevice);
   assert.equal(nativeAdapterRequests, 2);
   assert.equal(nativeDeviceRequests, 2);

   const readSnapshot = globalThis[SNAPSHOT_SYMBOL];
   assert.equal(typeof readSnapshot, "function");
   assert.equal(typeof globalThis[SHUTDOWN_SYMBOL], "function");
   assert.deepEqual(readSnapshot(), {
      protocol_version: 4,
      generation: 0,
      device_request_count: 0,
      live_device_count: 0,
      renderer_lease_count: 2,
      pending_renderer_initialization_count: 0,
      device_destroy_count: 0,
      incompatible_acquire_failure_count: 0,
      session_shutdown_count: 0,
      closed: false,
   });
   assert(Object.isFrozen(readSnapshot()));

   landingModule.releaseOxideWebGpuDeviceSession(landingLease);
   landingModule.releaseOxideWebGpuDeviceSession(landingLease);
   foundationModule.releaseOxideWebGpuDeviceSession(foundationLease);
   assert.equal(readSnapshot().renderer_lease_count, 0);
   assert.equal(nativeDeviceDestroys, 0);

   const failedLease = landingModule.acquireOxideWebGpuDeviceSession();
   const recoveredLease = foundationModule.acquireOxideWebGpuDeviceSession();
   await landingModule.waitForOxideWebGpuDeviceInitialization(failedLease);
   landingModule.releaseOxideWebGpuDeviceSession(failedLease);
   await foundationModule.waitForOxideWebGpuDeviceInitialization(recoveredLease);
   foundationModule.completeOxideWebGpuDeviceInitialization(recoveredLease);
   foundationModule.releaseOxideWebGpuDeviceSession(recoveredLease);
   assert.equal(readSnapshot().pending_renderer_initialization_count, 0);
   assert.equal(readSnapshot().renderer_lease_count, 0);

   const persistedPageHide = new Event("pagehide");
   Object.defineProperty(persistedPageHide, "persisted", { value: true });
   globalThis.dispatchEvent(persistedPageHide);
   assert.equal(readSnapshot().closed, false);

   const terminalPageHide = new Event("pagehide");
   Object.defineProperty(terminalPageHide, "persisted", { value: false });
   globalThis.dispatchEvent(terminalPageHide);
   assert.equal(readSnapshot().closed, true);
   assert.equal(readSnapshot().session_shutdown_count, 1);
   assert.equal(nativeDeviceDestroys, 0);
   assert.throws(
      () => landingModule.acquireOxideWebGpuDeviceSession(),
      /Oxide WebGPU page session is shut down/,
   );

   globalThis.dispatchEvent(terminalPageHide);
   globalThis[SHUTDOWN_SYMBOL]();
   assert.equal(readSnapshot().session_shutdown_count, 1);
   assert.equal(nativeDeviceDestroys, 0);
});
