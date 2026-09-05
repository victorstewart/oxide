import init, {OxideComparisonApp} from "/oxide/pkg/oxide_web_comparison.js";
import {installComparisonControl} from "/shared/control.js";

await init();
const app = await OxideComparisonApp.newAsync("oxide-canvas");
const adapter = {
   capabilities: () => JSON.parse(app.capabilities()),
   reset: async (scenarioId, seed, generation) => app.reset(scenarioId, BigInt(seed), generation),
   advance: async checkpointId => app.advance(checkpointId),
   ready: async () => app.ready(),
   snapshot: async () => JSON.parse(app.snapshot()),
   checkpoint: async checkpointId => JSON.parse(app.checkpoint(checkpointId)),
   teardown: async () => app.teardown(),
};
const control = installComparisonControl(adapter);
const scenario = new URL(location.href).searchParams.get("scenario") || "dashboard.mixed-static";
await control.reset(scenario, 0);
await control.ready();
app.start();
performance.mark("comparison-complete-state-commit", {detail: {generation: 1}});
