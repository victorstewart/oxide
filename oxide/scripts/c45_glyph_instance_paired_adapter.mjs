#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const binary = process.argv[2];
const output = process.argv[3];
if (!binary || !output) {
   throw new Error("usage: c45_glyph_instance_paired_adapter.mjs BINARY OUTPUT");
}

const suiteOutput = `${output}.suite.json`;
const result = spawnSync(binary, ["--run-suite", "--json-out", suiteOutput], {
   env: process.env,
   encoding: "utf8",
});
process.stdout.write(result.stdout);
process.stderr.write(result.stderr);
if (result.status !== 0) {
   throw new Error(`C45 glyph-instance suite failed with status ${result.status}`);
}

const caseId = "gpu.architecture.text.glyph_instances_1000";
const report = JSON.parse(readFileSync(suiteOutput, "utf8"));
const perfCase = report.cases?.find(entry => entry.id === caseId);
if (!perfCase) {
   throw new Error(`C45 glyph-instance suite omitted ${caseId}`);
}
const indexed = prefix => Object.entries(perfCase.metrics)
   .filter(([name, value]) => name.startsWith(prefix) && Number.isFinite(value))
   .sort(([a], [b]) => a.localeCompare(b))
   .map(([, value]) => value);
const primary = process.env.OXIDE_C45_PRIMARY ?? "frame";
const samples = indexed(primary === "encode" ? "c45_encode_ms_" : "c45_frame_ms_");
const warmups = indexed("c45_warmup_frame_ms_");
if (samples.length === 0 || warmups.length === 0) {
   throw new Error(`C45 glyph-instance suite omitted indexed ${primary} samples`);
}
const counterNames = new Set([
   "atlas_pages",
   "bitmap_and_sdf",
   "buffer_upload_bytes_avg",
   "bytes_per_glyph_instance",
   "draws_avg",
   "glyph_buffer_upload_bytes_per_instance",
   "glyph_instance_buffer_binds_avg",
   "glyph_instance_bytes_avg",
   "glyph_instances_avg",
   "labels",
]);
const counters = Object.fromEntries(Object.entries(perfCase.metrics).filter(([name]) =>
   counterNames.has(name)
));
writeFileSync(output, `${JSON.stringify({
   warmups,
   samples,
   encode_ms: indexed("c45_encode_ms_"),
   gpu_ms: indexed("c45_gpu_ms_"),
   counters,
}, null, 2)}\n`);
