#!/usr/bin/env node

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

function parseArgs(argv)
{
   let args = { cpu: "", backend: "", expect: "match", out: "" };
   for (let index = 0; index < argv.length; index += 1) {
      let arg = argv[index];
      let next = () => {
         index += 1;
         if (index >= argv.length) {
            throw new Error(`missing value for ${arg}`);
         }
         return argv[index];
      };
      if (arg === "--cpu") {
         args.cpu = next();
      } else if (arg === "--backend") {
         args.backend = next();
      } else if (arg === "--expect") {
         args.expect = next();
      } else if (arg === "--out") {
         args.out = next();
      } else {
         throw new Error(`unknown argument ${arg}`);
      }
   }
   if (!args.cpu || !args.backend || !["match", "mismatch"].includes(args.expect)) {
      throw new Error("usage: compare_id_mask_reference.mjs --cpu PATH --backend PATH --expect match|mismatch [--out PATH]");
   }
   return args;
}

function mismatchIndices(expected, actual)
{
   if (!Array.isArray(expected) || !Array.isArray(actual) || expected.length !== actual.length) {
      throw new Error("reference arrays differ in shape");
   }
   let indices = [];
   for (let index = 0; index < expected.length; index += 1) {
      if (JSON.stringify(expected[index]) !== JSON.stringify(actual[index])) {
         indices.push(index);
      }
   }
   return indices;
}

function main()
{
   let args = parseArgs(process.argv.slice(2));
   let cpu = JSON.parse(readFileSync(args.cpu, "utf8"));
   let backend = JSON.parse(readFileSync(args.backend, "utf8"));
   if (cpu.width !== backend.width || cpu.height !== backend.height) {
      throw new Error(`dimension mismatch CPU=${cpu.width}x${cpu.height} backend=${backend.width}x${backend.height}`);
   }
   let city = mismatchIndices(cpu.city, backend.city);
   let neighborhood = mismatchIndices(cpu.neighborhood, backend.neighborhood);
   let cityField = mismatchIndices(cpu.city_field, backend.city_field);
   let seamField = mismatchIndices(cpu.seam_field, backend.seam_field);
   let fieldsMatch = cityField.length === 0 && seamField.length === 0;
   if (city.length !== 0 || neighborhood.length !== 0) {
      throw new Error(`raster mismatch city=${city.length} neighborhood=${neighborhood.length}`);
   }
   if (args.expect === "match" && !fieldsMatch) {
      throw new Error(`field mismatch city=${cityField.length} seam=${seamField.length}`);
   }
   if (args.expect === "mismatch" && fieldsMatch) {
      throw new Error("expected the parent WebGPU field defect, but fields match");
   }
   let report = {
      width: cpu.width,
      height: cpu.height,
      raster_city_mismatches: city.length,
      raster_neighborhood_mismatches: neighborhood.length,
      city_field_mismatches: cityField.length,
      seam_field_mismatches: seamField.length,
      first_city_field_mismatches: cityField.slice(0, 16),
      first_seam_field_mismatches: seamField.slice(0, 16),
      expected: args.expect,
      decision: fieldsMatch ? "match" : "mismatch",
   };
   if (args.out) {
      mkdirSync(dirname(args.out), { recursive: true });
      writeFileSync(args.out, `${JSON.stringify(report, null, 2)}\n`);
   }
   console.log(JSON.stringify(report));
}

main();
