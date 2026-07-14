#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const binary = process.argv[2];
const output = process.argv[3];
if (!binary || !output) {
   throw new Error("usage: c43_text_paired_adapter.mjs BINARY OUTPUT");
}

const indexed = (perfCase, prefix) => Object.entries(perfCase.metrics)
   .filter(([name, value]) => name.startsWith(prefix) && Number.isFinite(value))
   .sort(([a], [b]) => a.localeCompare(b))
   .map(([, value]) => value);

const run = (suiteOutput, env) => {
   const result = spawnSync(binary, ["--run-suite", "--json-out", suiteOutput], {
      env,
      encoding: "utf8",
   });
   process.stdout.write(result.stdout);
   process.stderr.write(result.stderr);
   if (result.status !== 0) {
      throw new Error(`C43 suite failed with status ${result.status}`);
   }
   const report = JSON.parse(readFileSync(suiteOutput, "utf8"));
   const perfCase = report.cases?.find(entry => entry.id === "gpu.architecture.text.new_labels_200");
   if (!perfCase) {
      throw new Error("C43 suite omitted gpu.architecture.text.new_labels_200");
   }
   return perfCase;
};

const blocks = Number.parseInt(process.env.OXIDE_C43_PAIRED_BLOCKS ?? "1", 10);
const totalFrames = Number.parseInt(process.env.OXIDE_C43_METAL_FRAMES ?? "16", 10);
if (!Number.isInteger(blocks) || blocks < 1 || !Number.isInteger(totalFrames) || totalFrames % blocks !== 0) {
   throw new Error("C43 paired blocks must evenly divide the measured frame count");
}
const framesPerBlock = totalFrames / blocks;
const evidence = {
   warmups: [],
   samples: [],
   prepare_ms: [],
   encode_ms: [],
   gpu_ms: [],
   counters: null,
   topology: { blocks, frames_per_block: framesPerBlock, symmetric_conditioning_frames: 16 },
};
for (let block = 0; block < blocks; block += 1) {
   const tag = String(block).padStart(2, "0");
   run(`${output}.block-${tag}.conditioning.json`, {
      ...process.env,
      OXIDE_C43_TEXT_FRAME_SCOPED: "0",
      OXIDE_C43_METAL_FRAMES: "16",
      OXIDE_C43_RAW_SAMPLES: "",
   });
   const perfCase = run(`${output}.block-${tag}.suite.json`, {
      ...process.env,
      OXIDE_C43_METAL_FRAMES: String(framesPerBlock),
   });
   evidence.warmups.push(...indexed(perfCase, "c43_warmup_frame_ms_"));
   evidence.samples.push(...indexed(perfCase, "c43_frame_ms_"));
   evidence.prepare_ms.push(...indexed(perfCase, "c43_text_prepare_ms_"));
   evidence.encode_ms.push(...indexed(perfCase, "c43_encode_ms_"));
   evidence.gpu_ms.push(...indexed(perfCase, "c43_gpu_ms_"));
   const counters = Object.fromEntries(Object.entries(perfCase.metrics).filter(([name]) =>
      name.startsWith("atlas_")
      || name === "buffer_upload_bytes_avg"
      || name === "draws_avg"
      || name === "frame_scoped_preparation"
   ));
   if (evidence.counters && JSON.stringify(evidence.counters) !== JSON.stringify(counters)) {
      throw new Error("C43 block counters changed within one paired side");
   }
   evidence.counters = counters;
}
if (evidence.warmups.length === 0 || evidence.samples.length !== totalFrames) {
   throw new Error("C43 suite omitted indexed warmup or measured samples");
}
writeFileSync(output, `${JSON.stringify(evidence, null, 2)}\n`);
