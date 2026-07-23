import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const DEVICE_LABEL = "oxide-webgpu-shared-device-v1";
const SNAPSHOT_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.snapshot.v1");
const SHUTDOWN_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.shutdown.v1");

test("separate wasm modules reuse one page-session device across route transitions", async () => {
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
         let resolveLost;
         const lost = new Promise((resolve) => {
            resolveLost = resolve;
         });
         return Promise.resolve({
            lost,
            destroy()
            {
               nativeDeviceDestroys += 1;
               resolveLost();
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
   const landingAdapter = await globalThis.navigator.gpu.requestAdapter();
   const foundationAdapter = await globalThis.navigator.gpu.requestAdapter();
   const landingDevicePromise = landingAdapter.requestDevice({
      label: DEVICE_LABEL,
      requiredFeatures: ["timestamp-query"],
      requiredLimits: { maxBindGroups: 4, maxTextureDimension2D: 8_192 },
   });
   const foundationDevicePromise = foundationAdapter.requestDevice({
      label: DEVICE_LABEL,
      requiredFeatures: ["timestamp-query"],
      requiredLimits: { maxTextureDimension2D: 8_192, maxBindGroups: 4 },
   });
   assert.strictEqual(landingDevicePromise, foundationDevicePromise);
   const [landingDevice, foundationDevice] = await Promise.all([
      landingDevicePromise,
      foundationDevicePromise,
   ]);
   assert.strictEqual(landingDevice, foundationDevice);

   const readSnapshot = globalThis[SNAPSHOT_SYMBOL];
   assert.equal(typeof readSnapshot, "function");
   assert.equal(typeof globalThis[SHUTDOWN_SYMBOL], "function");
   assert.deepEqual(readSnapshot(), {
      protocol_version: 1,
      generation: 1,
      device_request_count: 1,
      live_device_count: 1,
      renderer_lease_count: 2,
      device_destroy_count: 0,
      incompatible_acquire_failure_count: 0,
      session_shutdown_count: 0,
      closed: false,
   });
   assert(Object.isFrozen(readSnapshot()));

   landingModule.releaseOxideWebGpuDeviceSession(landingLease);
   foundationModule.releaseOxideWebGpuDeviceSession(foundationLease);
   for (let transition = 0; transition < 128; transition += 1) {
      const module = transition % 2 === 0 ? landingModule : foundationModule;
      const lease = module.acquireOxideWebGpuDeviceSession();
      const adapter = await globalThis.navigator.gpu.requestAdapter();
      const device = await adapter.requestDevice({
         label: DEVICE_LABEL,
         requiredFeatures: ["timestamp-query"],
         requiredLimits: { maxBindGroups: 4, maxTextureDimension2D: 8_192 },
      });
      assert.strictEqual(device, landingDevice);
      module.releaseOxideWebGpuDeviceSession(lease);
   }
   assert.equal(nativeAdapterRequests, 130);
   assert.equal(nativeDeviceRequests, 1);
   assert.deepEqual(
      {
         requests: readSnapshot().device_request_count,
         live: readSnapshot().live_device_count,
         leases: readSnapshot().renderer_lease_count,
         destroys: readSnapshot().device_destroy_count,
      },
      { requests: 1, live: 1, leases: 0, destroys: 0 },
   );

   const incompatibleLease = foundationModule.acquireOxideWebGpuDeviceSession();
   const incompatibleAdapter = await globalThis.navigator.gpu.requestAdapter();
   await assert.rejects(
      incompatibleAdapter.requestDevice({
         label: DEVICE_LABEL,
         requiredFeatures: ["timestamp-query"],
         requiredLimits: { maxBindGroups: 8, maxTextureDimension2D: 8_192 },
      }),
      /incompatible Oxide WebGPU device requirements/,
   );
   assert.equal(nativeAdapterRequests, 131);
   foundationModule.releaseOxideWebGpuDeviceSession(incompatibleLease);
   assert.equal(readSnapshot().incompatible_acquire_failure_count, 1);

   const persistedPageHide = new Event("pagehide");
   Object.defineProperty(persistedPageHide, "persisted", { value: true });
   globalThis.dispatchEvent(persistedPageHide);
   assert.equal(readSnapshot().closed, false);
   assert.equal(readSnapshot().live_device_count, 1);

   const terminalPageHide = new Event("pagehide");
   Object.defineProperty(terminalPageHide, "persisted", { value: false });
   globalThis.dispatchEvent(terminalPageHide);
   assert.equal(nativeDeviceDestroys, 1);
   assert.deepEqual(
      {
         requests: readSnapshot().device_request_count,
         live: readSnapshot().live_device_count,
         leases: readSnapshot().renderer_lease_count,
         destroys: readSnapshot().device_destroy_count,
         shutdowns: readSnapshot().session_shutdown_count,
         closed: readSnapshot().closed,
      },
      { requests: 1, live: 0, leases: 0, destroys: 1, shutdowns: 1, closed: true },
   );
   assert.throws(
      () => landingModule.acquireOxideWebGpuDeviceSession(),
      /Oxide WebGPU page session is shut down/,
   );
   const repeatedTerminalPageHide = new Event("pagehide");
   Object.defineProperty(repeatedTerminalPageHide, "persisted", { value: false });
   globalThis.dispatchEvent(repeatedTerminalPageHide);
   assert.equal(nativeDeviceDestroys, 1);
   assert.equal(readSnapshot().session_shutdown_count, 1);
   globalThis[SHUTDOWN_SYMBOL]();
   assert.equal(nativeDeviceDestroys, 1);
   assert.equal(readSnapshot().session_shutdown_count, 1);
});
