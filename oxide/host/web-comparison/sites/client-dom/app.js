import {installComparisonControl} from "/shared/control.js";
import {DomSceneAdapter} from "/shared/scenes.js";

const root = document.getElementById("comparison-root");
const adapter = new DomSceneAdapter(root, "client-dom.production");
const control = installComparisonControl(adapter);
const scenario = new URL(location.href).searchParams.get("scenario") || "dashboard.mixed-static";
await control.reset(scenario, 0);
await control.ready();
performance.mark("comparison-complete-state-commit", {detail: {generation: 1}});
