#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const binary = process.argv[2];
const output = process.argv[3];
if (!binary || !output) {
   throw new Error("usage: c44_text_atlas_paired_adapter.mjs BINARY OUTPUT");
}

const suiteOutput = `${output}.suite.json`;
const result = spawnSync(binary, ["--run-suite", "--json-out", suiteOutput], {
   env: process.env,
   encoding: "utf8",
});
process.stdout.write(result.stdout);
process.stderr.write(result.stderr);
if (result.status !== 0) {
   throw new Error(`C44 suite failed with status ${result.status}`);
}

const report = JSON.parse(readFileSync(suiteOutput, "utf8"));
const perfCase = report.cases?.find(
   entry => entry.id === "gpu.architecture.text.paged_atlas_locality",
);
if (!perfCase) {
   throw new Error("C44 suite omitted gpu.architecture.text.paged_atlas_locality");
}
const indexed = prefix => Object.entries(perfCase.metrics)
   .filter(([name, value]) => name.startsWith(prefix) && Number.isFinite(value))
   .sort(([a], [b]) => a.localeCompare(b))
   .map(([, value]) => value);
const samples = indexed("c44_frame_ms_");
if (samples.length === 0) {
   throw new Error("C44 suite omitted indexed frame samples");
}
const counters = Object.fromEntries(Object.entries(perfCase.metrics).filter(([name]) =>
   name === "paged_atlas"
   || name === "glyph_pages"
   || name === "atlas_resident_bytes"
   || name === "atlas_upload_bytes_avg"
   || name === "invalidated_chunks_avg"
   || name === "prepared_cache_hits_avg"
   || name === "prepared_cache_misses_avg"
   || name === "chunks_prepared_avg"
   || name === "draws_avg"
));
writeFileSync(output, `${JSON.stringify({
   warmups: indexed("c44_warmup_frame_ms_"),
   samples,
   encode_ms: indexed("c44_encode_ms_"),
   gpu_ms: indexed("c44_gpu_ms_"),
   counters,
}, null, 2)}\n`);
