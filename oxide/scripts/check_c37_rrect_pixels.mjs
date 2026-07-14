#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";
import { inflateSync } from "node:zlib";

let [parentPath, candidatePath, dprText, outPath] = process.argv.slice(2);
let dpr = Number(dprText);
if (!parentPath || !candidatePath || ![1, 2, 3].includes(dpr)) {
   throw new Error("usage: check_c37_rrect_pixels.mjs PARENT.png CANDIDATE.png DPR [OUT.json]");
}

function paeth(a, b, c)
{
   let p = a + b - c;
   let pa = Math.abs(p - a);
   let pb = Math.abs(p - b);
   let pc = Math.abs(p - c);
   if (pa <= pb && pa <= pc) {
      return a;
   }
   return pb <= pc ? b : c;
}

function loadPngRgba(path)
{
   let bytes = readFileSync(path);
   let signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
   if (bytes.length < 8 || bytes.subarray(0, 8).compare(signature) !== 0) {
      throw new Error(`${path} is not a PNG`);
   }
   let width = 0;
   let height = 0;
   let colorType = 0;
   let idat = [];
   for (let position = 8; position < bytes.length;) {
      let length = bytes.readUInt32BE(position);
      let kind = bytes.subarray(position + 4, position + 8).toString("ascii");
      let chunk = bytes.subarray(position + 8, position + 8 + length);
      position += 12 + length;
      if (kind === "IHDR") {
         width = chunk.readUInt32BE(0);
         height = chunk.readUInt32BE(4);
         colorType = chunk[9];
         if (chunk[8] !== 8 || (colorType !== 2 && colorType !== 6)
            || chunk[10] !== 0 || chunk[11] !== 0 || chunk[12] !== 0) {
            throw new Error(`${path} uses unsupported PNG encoding`);
         }
      } else if (kind === "IDAT") {
         idat.push(chunk);
      } else if (kind === "IEND") {
         break;
      }
   }
   let channels = colorType === 6 ? 4 : 3;
   let stride = width * channels;
   let source = inflateSync(Buffer.concat(idat));
   let rgba = Buffer.alloc(width * height * 4);
   let previous = Buffer.alloc(stride);
   let sourceOffset = 0;
   let outputOffset = 0;
   for (let y = 0; y < height; y += 1) {
      let filter = source[sourceOffset++];
      let row = Buffer.alloc(stride);
      for (let x = 0; x < stride; x += 1) {
         let left = x >= channels ? row[x - channels] : 0;
         let up = previous[x];
         let upLeft = x >= channels ? previous[x - channels] : 0;
         let value = source[sourceOffset++];
         if (filter === 0) {
            row[x] = value;
         } else if (filter === 1) {
            row[x] = (value + left) & 255;
         } else if (filter === 2) {
            row[x] = (value + up) & 255;
         } else if (filter === 3) {
            row[x] = (value + Math.floor((left + up) / 2)) & 255;
         } else if (filter === 4) {
            row[x] = (value + paeth(left, up, upLeft)) & 255;
         } else {
            throw new Error(`${path} has unsupported PNG filter ${filter}`);
         }
      }
      for (let x = 0; x < width; x += 1) {
         let input = x * channels;
         rgba[outputOffset++] = row[input];
         rgba[outputOffset++] = row[input + 1];
         rgba[outputOffset++] = row[input + 2];
         rgba[outputOffset++] = channels === 4 ? row[input + 3] : 255;
      }
      previous = row;
   }
   return { width, height, rgba };
}

function radiiFor(index)
{
   switch (index % 8) {
      case 0: return [-4, 0, 8, 40];
      case 1: return [40, 8, 0, -4];
      case 2: return [0, 0, 0, 0];
      case 3: return [0.25, 3, 12, 40];
      case 4: return [12, 12, 12, 12];
      case 5: return [64, 64, 64, 64];
      case 6: return [2, 6, 12, 18];
      default: return [22, 0.5, 22, 0.5];
   }
}

function rrectDistance(x, y, index)
{
   let column = index % 8;
   let row = Math.floor(index / 8);
   let width = (index & 1) === 0 ? 25 : 15;
   let height = (index & 2) === 0 ? 25 : 19;
   let localX = x - (column * 32 + 3);
   let localY = y - (row * 32 + 3);
   let centerX = width * 0.5;
   let centerY = height * 0.5;
   let radii = radiiFor(index);
   let radiusIndex = localY >= centerY ? (localX >= centerX ? 2 : 3) : (localX >= centerX ? 1 : 0);
   let radius = Math.max(0, Math.min(Math.min(centerX, centerY), radii[radiusIndex]));
   let qx = Math.abs(localX - centerX) - (centerX - radius);
   let qy = Math.abs(localY - centerY) - (centerY - radius);
   return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - radius;
}

let parent = loadPngRgba(parentPath);
let candidate = loadPngRgba(candidatePath);
if (parent.width !== candidate.width || parent.height !== candidate.height) {
   throw new Error(`capture dimensions differ: ${parent.width}x${parent.height} vs ${candidate.width}x${candidate.height}`);
}
let scale = dpr;
if (parent.width < 256 * dpr || parent.height < 256 * dpr) {
   throw new Error(`expected at least a ${256 * dpr}x${256 * dpr} DPR${dpr} capture, got ${parent.width}x${parent.height}`);
}

let changedPixels = 0;
let interiorMismatches = 0;
let maxChannelError = 0;
let maxBoundaryDistancePx = 0;
let maxBoundaryCenterDistancePx = 0;
for (let y = 0; y < parent.height; y += 1) {
   for (let x = 0; x < parent.width; x += 1) {
      let offset = (y * parent.width + x) * 4;
      let changed = false;
      for (let channel = 0; channel < 4; channel += 1) {
         let error = Math.abs(parent.rgba[offset + channel] - candidate.rgba[offset + channel]);
         maxChannelError = Math.max(maxChannelError, error);
         changed = changed || error !== 0;
      }
      if (!changed) {
         continue;
      }
      changedPixels += 1;
      let logicalX = (x + 0.5) / scale;
      let logicalY = (y + 0.5) / scale;
      let nearest = Number.POSITIVE_INFINITY;
      for (let index = 0; index < 64; index += 1) {
         nearest = Math.min(nearest, Math.abs(rrectDistance(logicalX, logicalY, index)) * scale);
      }
      maxBoundaryCenterDistancePx = Math.max(maxBoundaryCenterDistancePx, nearest);
      let pixelFootprintDistance = Math.max(0, nearest - Math.SQRT1_2);
      maxBoundaryDistancePx = Math.max(maxBoundaryDistancePx, pixelFootprintDistance);
      if (pixelFootprintDistance > 1.0001) {
         interiorMismatches += 1;
      }
   }
}

let report = {
   parent: parentPath,
   candidate: candidatePath,
   dpr,
   width: parent.width,
   height: parent.height,
   changed_pixels: changedPixels,
   interior_mismatches: interiorMismatches,
   max_boundary_center_distance_px: maxBoundaryCenterDistancePx,
   max_boundary_distance_px: maxBoundaryDistancePx,
   max_channel_error: maxChannelError,
   tolerance: "differences allowed only when the pixel footprint is within one physical pixel of an analytic RRect boundary",
};
let json = `${JSON.stringify(report, null, 2)}\n`;
if (outPath) {
   writeFileSync(outPath, json);
}
process.stdout.write(json);
if (interiorMismatches !== 0) {
   process.exitCode = 1;
}
