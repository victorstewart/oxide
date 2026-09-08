import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const SNAPSHOT_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.snapshot.v1");
const SHUTDOWN_SYMBOL = Symbol.for("oxide.renderer-web.webgpu-device-session.shutdown.v1");

test("page owner serializes renderer initialization and closes terminal work", async () => {
   const events = new EventTarget();
   globalThis.addEventListener = events.addEventListener.bind(events);
   globalThis.dispatchEvent = events.dispatchEvent.bind(events);

   const source = await readFile(
      new URL("../src/wasm/webgpu_device_session.js", import.meta.url),
      "utf8",
   );
   const importCopy = (name) => import(
      `data:text/javascript;base64,${Buffer.from(`${source}\n// ${name}`).toString("base64")}`
   );
   const owner = await importCopy("foundation and topomap owner");
   const foreign = await importCopy("foreign compiled module");
   const readSnapshot = globalThis[SNAPSHOT_SYMBOL];

   const first = owner.acquireOxideWebGpuDeviceSession();
   const second = owner.acquireOxideWebGpuDeviceSession();
   assert.throws(
      () => foreign.acquireOxideWebGpuDeviceSession(),
      /belongs to another compiled WASM module/,
   );
   await owner.waitForOxideWebGpuDeviceInitialization(first);
   let secondReady = false;
   const secondWait = owner.waitForOxideWebGpuDeviceInitialization(second)
      .then(() => { secondReady = true; });
   await Promise.resolve();
   assert.equal(secondReady, false);

   // A failed constructor releases its lease and must unblock the next owner.
   owner.releaseOxideWebGpuDeviceSession(first);
   await secondWait;
   owner.completeOxideWebGpuDeviceInitialization(second);
   owner.releaseOxideWebGpuDeviceSession(second);

   // A successful constructor explicitly completes before the following one.
   const third = owner.acquireOxideWebGpuDeviceSession();
   const fourth = owner.acquireOxideWebGpuDeviceSession();
   await owner.waitForOxideWebGpuDeviceInitialization(third);
   let fourthReady = false;
   const fourthWait = owner.waitForOxideWebGpuDeviceInitialization(fourth)
      .then(() => { fourthReady = true; });
   await Promise.resolve();
   assert.equal(fourthReady, false);
   owner.completeOxideWebGpuDeviceInitialization(third);
   await fourthWait;
   owner.completeOxideWebGpuDeviceInitialization(fourth);
   owner.releaseOxideWebGpuDeviceSession(third);
   owner.releaseOxideWebGpuDeviceSession(fourth);

   assert.deepEqual(readSnapshot(), {
      protocol_version: 10,
      renderer_lease_count: 0,
      pending_renderer_initialization_count: 0,
      incompatible_module_failure_count: 1,
      session_shutdown_count: 0,
      closed: false,
   });
   assert(Object.isFrozen(readSnapshot()));

   const persistedPageHide = new Event("pagehide");
   Object.defineProperty(persistedPageHide, "persisted", { value: true });
   globalThis.dispatchEvent(persistedPageHide);
   assert.equal(readSnapshot().closed, false);

   const terminalFirst = owner.acquireOxideWebGpuDeviceSession();
   const terminalSecond = owner.acquireOxideWebGpuDeviceSession();
   await owner.waitForOxideWebGpuDeviceInitialization(terminalFirst);
   const terminalSecondWait = owner.waitForOxideWebGpuDeviceInitialization(terminalSecond);
   const terminalPageHide = new Event("pagehide");
   Object.defineProperty(terminalPageHide, "persisted", { value: false });
   globalThis.dispatchEvent(terminalPageHide);
   await assert.rejects(terminalSecondWait, /page shutdown/);
   assert.deepEqual(readSnapshot(), {
      protocol_version: 10,
      renderer_lease_count: 2,
      pending_renderer_initialization_count: 0,
      incompatible_module_failure_count: 1,
      session_shutdown_count: 1,
      closed: true,
   });
   owner.releaseOxideWebGpuDeviceSession(terminalFirst);
   owner.releaseOxideWebGpuDeviceSession(terminalSecond);
   assert.throws(
      () => owner.acquireOxideWebGpuDeviceSession(),
      /page session is shut down/,
   );
   globalThis[SHUTDOWN_SYMBOL]();
   assert.equal(readSnapshot().session_shutdown_count, 1);
});
