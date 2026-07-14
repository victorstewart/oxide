#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const binary = process.argv[2];
const output = process.argv[3];
if (!binary || !output) {
   throw new Error("usage: c44_text_cpu_paired_adapter.mjs BINARY OUTPUT");
}

const caseId = process.env.OXIDE_C44_CASE_ID ?? "cpu.architecture.text.warm_labels_1000";
const repeats = Number.parseInt(process.env.OXIDE_C44_CPU_REPEATS ?? "5", 10);
if (!Number.isInteger(repeats) || repeats < 1) {
   throw new Error("OXIDE_C44_CPU_REPEATS must be positive");
}
const medians = [];
let counters = null;
for (let run = 0; run <= repeats; run += 1) {
   const suiteOutput = `${output}.suite-${String(run).padStart(2, "0")}.json`;
   const result = spawnSync(binary, ["--run-suite", "--json-out", suiteOutput], {
      env: process.env,
      encoding: "utf8",
   });
   process.stdout.write(result.stdout);
   process.stderr.write(result.stderr);
   if (result.status !== 0) {
      throw new Error(`C44 CPU suite failed with status ${result.status}`);
   }
   const report = JSON.parse(readFileSync(suiteOutput, "utf8"));
   const perfCase = report.cases?.find(entry => entry.id === caseId);
   if (!perfCase || !Number.isFinite(perfCase.median)) {
      throw new Error(`C44 CPU suite omitted ${caseId}`);
   }
   medians.push(perfCase.median);
   counters = Object.fromEntries(Object.entries(perfCase.metrics).filter(([name]) =>
      name.startsWith("atlas_")
      || name.startsWith("glyph_")
      || name.startsWith("layout_")
      || name === "rasterizations"
      || name === "shaping_calls"
   ));
}
writeFileSync(output, `${JSON.stringify({
   warmups: medians.slice(0, 1),
   samples: medians.slice(1),
   counters,
}, null, 2)}\n`);
