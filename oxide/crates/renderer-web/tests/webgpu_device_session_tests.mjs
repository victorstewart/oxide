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
   let fallbackAdapterFailures = 1;
   let deviceFailures = 0;
   const events = new EventTarget();
   globalThis.addEventListener = events.addEventListener.bind(events);
   globalThis.dispatchEvent = events.dispatchEvent.bind(events);

   class MockGpuAdapter
   {
      requestDevice()
      {
         nativeDeviceRequests += 1;
         if (deviceFailures > 0) {
            deviceFailures -= 1;
            return Promise.reject(new DOMException(
               "A valid external Instance reference no longer exists.",
               "OperationError",
            ));
         }
         let resolveLost;
         const lost = new Promise((resolve) => {
            resolveLost = resolve;
         });
         return Promise.resolve({
            lost,
            loseForTest()
            {
               resolveLost();
            },
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
      requestAdapter(options)
      {
         nativeAdapterRequests += 1;
         if (options?.forceFallbackAdapter && fallbackAdapterFailures > 0) {
            fallbackAdapterFailures -= 1;
            return Promise.reject(new Error("transient adapter discovery failure"));
         }
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
   const landingAdapterPromise = globalThis.navigator.gpu.requestAdapter({
      powerPreference: "high-performance",
      forceFallbackAdapter: false,
   });
   const foundationAdapterPromise = globalThis.navigator.gpu.requestAdapter({
      forceFallbackAdapter: false,
      powerPreference: "high-performance",
   });
   assert.strictEqual(landingAdapterPromise, foundationAdapterPromise);
   const [landingAdapter, foundationAdapter] = await Promise.all([
      landingAdapterPromise,
      foundationAdapterPromise,
   ]);
   assert.strictEqual(landingAdapter, foundationAdapter);
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
      protocol_version: 6,
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
      const adapter = await globalThis.navigator.gpu.requestAdapter({
         powerPreference: "high-performance",
         forceFallbackAdapter: false,
      });
      const device = await adapter.requestDevice({
         label: DEVICE_LABEL,
         requiredFeatures: ["timestamp-query"],
         requiredLimits: { maxBindGroups: 4, maxTextureDimension2D: 8_192 },
      });
      assert.strictEqual(device, landingDevice);
      module.releaseOxideWebGpuDeviceSession(lease);
   }
   assert.equal(nativeAdapterRequests, 1);
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

   const fallbackOptions = {
      powerPreference: "high-performance",
      forceFallbackAdapter: true,
   };
   await assert.rejects(
      globalThis.navigator.gpu.requestAdapter(fallbackOptions),
      /transient adapter discovery failure/,
   );
   assert(await globalThis.navigator.gpu.requestAdapter(fallbackOptions));
   assert.equal(nativeAdapterRequests, 3);

   const incompatibleLease = foundationModule.acquireOxideWebGpuDeviceSession();
   const lowPowerAdapterPromise = globalThis.navigator.gpu.requestAdapter({
      powerPreference: "low-power",
      forceFallbackAdapter: false,
   });
   assert.strictEqual(
      lowPowerAdapterPromise,
      globalThis.navigator.gpu.requestAdapter({
         forceFallbackAdapter: false,
         powerPreference: "low-power",
      }),
   );
   const lowPowerAdapter = await lowPowerAdapterPromise;
   await assert.rejects(
      lowPowerAdapter.requestDevice({
         label: DEVICE_LABEL,
         requiredFeatures: ["timestamp-query"],
         requiredLimits: { maxBindGroups: 4, maxTextureDimension2D: 8_192 },
      }),
      /incompatible Oxide WebGPU adapter requirements/,
   );
   const incompatibleAdapter = await globalThis.navigator.gpu.requestAdapter({
      powerPreference: "high-performance",
      forceFallbackAdapter: false,
   });
   await assert.rejects(
      incompatibleAdapter.requestDevice({
         label: DEVICE_LABEL,
         requiredFeatures: ["timestamp-query"],
         requiredLimits: { maxBindGroups: 8, maxTextureDimension2D: 8_192 },
      }),
      /incompatible Oxide WebGPU device requirements/,
   );
   assert.equal(nativeAdapterRequests, 4);
   foundationModule.releaseOxideWebGpuDeviceSession(incompatibleLease);
   assert.equal(readSnapshot().incompatible_acquire_failure_count, 2);

   landingDevice.loseForTest();
   await Promise.resolve();
   assert.equal(readSnapshot().generation, 0);
   assert.equal(readSnapshot().live_device_count, 0);
   const recoveredLease = landingModule.acquireOxideWebGpuDeviceSession();
   const recoveredAdapter = await globalThis.navigator.gpu.requestAdapter({
      powerPreference: "high-performance",
      forceFallbackAdapter: false,
   });
   deviceFailures = 8;
   for (let failure = 0; failure < 8; failure += 1) {
      await assert.rejects(
         recoveredAdapter.requestDevice({
            label: DEVICE_LABEL,
            requiredFeatures: ["timestamp-query"],
            requiredLimits: { maxBindGroups: 4, maxTextureDimension2D: 8_192 },
         }),
         /valid external Instance reference no longer exists/,
      );
      assert.equal(readSnapshot().live_device_count, 0);
      assert.equal(readSnapshot().generation, 2);
   }
   const recoveredDevice = await recoveredAdapter.requestDevice({
      label: DEVICE_LABEL,
      requiredFeatures: ["timestamp-query"],
      requiredLimits: { maxBindGroups: 4, maxTextureDimension2D: 8_192 },
   });
   assert.notStrictEqual(recoveredDevice, landingDevice);
   assert.equal(nativeAdapterRequests, 4);
   assert.equal(nativeDeviceRequests, 10);
   assert.equal(readSnapshot().device_request_count, 10);
   assert.equal(readSnapshot().generation, 2);
   assert.equal(readSnapshot().live_device_count, 1);
   landingModule.releaseOxideWebGpuDeviceSession(recoveredLease);

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
      { requests: 10, live: 0, leases: 0, destroys: 1, shutdowns: 1, closed: true },
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
