#!/usr/bin/env node

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const inputDir = process.argv[2];
const output = process.argv[3];
const parentFull = process.argv[4];
const candidateFull = process.argv[5];
if (!inputDir || !output || !parentFull || !candidateFull) {
   throw new Error("usage: c46_webgpu_glyph_paired_adapter.mjs INPUT_DIR OUTPUT PARENT_FULL CANDIDATE_FULL");
}

function metrics(value)
{
   return Object.fromEntries(String(value).split(";").filter(Boolean).map(field => {
      const split = field.indexOf("=");
      if (split < 1) {
         throw new Error(`invalid metric field ${field}`);
      }
      const name = field.slice(0, split);
      const raw = field.slice(split + 1);
      const number = Number(raw);
      return [name, Number.isFinite(number) ? number : raw];
   }));
}

function quantile(values, fraction)
{
   const sorted = [...values].sort((a, b) => a - b);
   return sorted[Math.round((sorted.length - 1) * fraction)];
}

function distribution(values)
{
   return {
      samples: values.length,
      p50: quantile(values, 0.50),
      p95: quantile(values, 0.95),
      p99: quantile(values, 0.99),
      min: Math.min(...values),
      max: Math.max(...values),
      avg: values.reduce((sum, value) => sum + value, 0) / values.length,
   };
}

function bootstrapMedian(values, iterations = 20_000)
{
   let state = 0xc46a011c;
   const next = () => {
      state ^= state << 13;
      state ^= state >>> 17;
      state ^= state << 5;
      return (state >>> 0) / 0x1_0000_0000;
   };
   const medians = new Array(iterations);
   const sample = new Array(values.length);
   for (let iteration = 0; iteration < iterations; iteration += 1) {
      for (let index = 0; index < values.length; index += 1) {
         sample[index] = values[Math.floor(next() * values.length)];
      }
      medians[iteration] = quantile(sample, 0.50);
   }
   return {
      iterations,
      low_95: quantile(medians, 0.025),
      high_95: quantile(medians, 0.975),
   };
}

function pairedMetric(pairs, name)
{
   const parent = pairs.map(pair => pair.parent[name]);
   const candidate = pairs.map(pair => pair.candidate[name]);
   const speedups = pairs.map(pair =>
      (pair.parent[name] - pair.candidate[name]) / pair.parent[name] * 100.0
   );
   return {
      parent: distribution(parent),
      candidate: distribution(candidate),
      paired_speedup_pct: distribution(speedups),
      paired_median_speedup_ci_95_pct: bootstrapMedian(speedups),
      candidate_wins: speedups.filter(value => value > 0.0).length,
   };
}

function pacing(raw)
{
   const deltas = raw.raf_deltas_ms;
   const cpu = raw.cpu_submit_ms;
   const gpu = raw.gpu_timestamp_samples.map(sample => Number(sample.total_ns) / 1_000_000.0);
   if (raw.frames !== 2_000 || deltas.length !== 2_000 || cpu.length !== 2_000 || gpu.length < 2_000) {
      throw new Error("C46 full report omitted its 2,000-frame RAF/GPU population");
   }
   const budget120 = 1_000.0 / 120.0;
   return {
      raf_ms: distribution(deltas),
      cpu_submit_ms: distribution(cpu),
      gpu_ms: distribution(gpu),
      missed_frames_120hz: deltas.filter(value => value > budget120).length,
      hitch_frames_120hz: deltas.filter(value => value > budget120 * 2.0).length,
   };
}

const names = readdirSync(inputDir).filter(name => /^\d\d-(parent|candidate)\.json$/.test(name));
const pairIds = [...new Set(names.map(name => name.slice(0, 2)))].sort();
const pairs = pairIds.map(id => {
   const parent = JSON.parse(readFileSync(join(inputDir, `${id}-parent.json`), "utf8"));
   const candidate = JSON.parse(readFileSync(join(inputDir, `${id}-candidate.json`), "utf8"));
   return {
      id,
      parent: metrics(parent.glyph_run_current),
      candidate: metrics(candidate.glyph_run_current),
   };
});
if (pairs.length !== 15) {
   throw new Error(`expected 15 balanced C46 pairs, found ${pairs.length}`);
}
for (const pair of pairs) {
   if (pair.parent.current_glyph_quads !== 512
      || pair.candidate.current_glyph_quads !== 512
      || pair.parent.current_sdf_glyph_quads !== 256
      || pair.candidate.current_sdf_glyph_quads !== 256
      || pair.parent.current_draw_items !== 65
      || pair.candidate.current_draw_items !== 3
      || pair.candidate.current_glyph_instances !== 512
      || pair.candidate.current_glyph_instance_bytes !== 18_432
      || pair.candidate.current_buffer_upload_bytes !== 18_468
      || pair.parent.current_buffer_upload_bytes !== 47_140) {
      throw new Error(`C46 invariant mismatch in pair ${pair.id}`);
   }
}

const primary = pairedMetric(pairs, "current_p50_ms");
const report = {
   case: "c46-webgpu-glyph-instances",
   pair_count: pairs.length,
   order: "balanced alternating fresh Chrome processes",
   workload: "512 glyphs in 64 source runs; 256 bitmap and 256 SDF glyphs",
   metrics: {
      cpu_submit_p50_ms: primary,
      cpu_submit_p95_ms: pairedMetric(pairs, "current_p95_ms"),
      cpu_submit_p99_ms: pairedMetric(pairs, "current_p99_ms"),
      cpu_submit_peak_ms: pairedMetric(pairs, "current_peak_ms"),
      gpu_timestamp_total_ns: pairedMetric(pairs, "current_gpu_timestamp_total_ns"),
   },
   counters: {
      parent: {
         draws: 65,
         draw_items: 65,
         glyph_quads: 512,
         buffer_upload_bytes: 47_140,
      },
      candidate: {
         draws: 3,
         draw_items: 3,
         glyph_quads: 512,
         glyph_instances: 512,
         glyph_instance_bytes: 18_432,
         buffer_upload_bytes: 18_468,
      },
   },
   raf: {
      parent: pacing(JSON.parse(readFileSync(parentFull, "utf8")).raf_frame_perf),
      candidate: pacing(JSON.parse(readFileSync(candidateFull, "utf8")).raf_frame_perf),
      note: "The production RAF row exercises the normal app text path; the 512-glyph isolated row supplies the controlled lowering A/B.",
   },
   policy: {
      minimum_paired_median_speedup_pct: 5.0,
      require_ci_low_above_zero: true,
      accepted: primary.paired_speedup_pct.p50 >= 5.0
         && primary.paired_median_speedup_ci_95_pct.low_95 > 0.0,
   },
};
writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
