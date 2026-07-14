#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const binary = process.argv[2];
const output = process.argv[3];
if (!binary || !output) {
   throw new Error("usage: c44_text_production_paired_adapter.mjs BINARY OUTPUT");
}

const suiteOutput = `${output}.suite.json`;
const result = spawnSync(binary, ["--run-suite", "--json-out", suiteOutput], {
   env: process.env,
   encoding: "utf8",
});
process.stdout.write(result.stdout);
process.stderr.write(result.stderr);
if (result.status !== 0) {
   throw new Error(`C44 production suite failed with status ${result.status}`);
}

const caseId = process.env.OXIDE_C44_CASE_ID ?? "gpu.architecture.text.new_labels_200";
const report = JSON.parse(readFileSync(suiteOutput, "utf8"));
const perfCase = report.cases?.find(entry => entry.id === caseId);
if (!perfCase) {
   throw new Error(`C44 production suite omitted ${caseId}`);
}
const indexed = prefix => Object.entries(perfCase.metrics)
   .filter(([name, value]) => name.startsWith(prefix) && Number.isFinite(value))
   .sort(([a], [b]) => a.localeCompare(b))
   .map(([, value]) => value);
const samples = indexed("c43_frame_ms_");
const warmups = indexed("c43_warmup_frame_ms_");
if (samples.length === 0 || warmups.length === 0) {
   throw new Error("C44 production suite omitted indexed frame samples");
}
const counters = Object.fromEntries(Object.entries(perfCase.metrics).filter(([name]) =>
   name.startsWith("atlas_")
   || name === "buffer_upload_bytes_avg"
   || name === "draws_avg"
));
writeFileSync(output, `${JSON.stringify({
   warmups,
   samples,
   prepare_ms: indexed("c43_text_prepare_ms_"),
   encode_ms: indexed("c43_encode_ms_"),
   gpu_ms: indexed("c43_gpu_ms_"),
   counters,
}, null, 2)}\n`);
