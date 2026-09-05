import {installComparisonControl} from "/shared/control.js";
import {DomSceneAdapter} from "/shared/scenes.js";

const root = document.getElementById("comparison-root");
const adapter = new DomSceneAdapter(root, "html-first.production", true);
const scenario = root.dataset.scenarioId;
installComparisonControl(adapter, 1);
await adapter.adoptServerRendered(scenario, 0);
performance.mark("comparison-complete-state-commit", {detail: {generation: 1}});
