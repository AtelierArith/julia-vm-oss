import { readFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";

const [wasmPath] = process.argv.slice(2);
if (wasmPath === undefined) {
  throw new Error("usage: node benchmarks/wasm_aot_rgba.mjs <module.wasm>");
}

const bytes = await readFile(wasmPath);
const compileStarted = performance.now();
const module = await WebAssembly.compile(bytes);
const compileMs = performance.now() - compileStarted;
const instantiateStarted = performance.now();
const instance = await WebAssembly.instantiate(module, {});
const instantiateMs = performance.now() - instantiateStarted;
const entry = Object.entries(instance.exports).find(
  ([name, value]) => name.includes("invert_rgba") && typeof value === "function",
)?.[1];
if (typeof entry !== "function") {
  throw new Error("generated module does not export invert_rgba");
}

const width = 888;
const height = 862;
const length = width * height * 4;
const descriptor = 32;
const pointer = 64;
const memory = instance.exports.memory;
const pixels = new Uint8Array(memory.buffer, pointer, length);
for (let index = 0; index < length; index += 4) {
  pixels[index] = 10;
  pixels[index + 1] = 20;
  pixels[index + 2] = 30;
  pixels[index + 3] = 255;
}
const descriptorView = new DataView(memory.buffer);
descriptorView.setInt32(descriptor, 1, true);
descriptorView.setInt32(descriptor + 4, pointer, true);
descriptorView.setInt32(descriptor + 8, length, true);
descriptorView.setInt32(descriptor + 12, 1, true);
descriptorView.setInt32(descriptor + 16, 1, true);
entry(descriptor);

const samples = [];
for (let iteration = 0; iteration < 20; iteration += 1) {
  const started = performance.now();
  entry(descriptor);
  samples.push(performance.now() - started);
}
samples.sort((left, right) => left - right);
const median = samples[Math.floor(samples.length / 2)];
const p95 = samples[Math.ceil(samples.length * 0.95) - 1];
if (pixels[3] !== 255 || pixels[length - 1] !== 255) {
  throw new Error("RGBA benchmark changed alpha bytes");
}
console.log(
  `compile_ms=${compileMs.toFixed(3)} instantiate_ms=${instantiateMs.toFixed(3)} median_ms=${median.toFixed(3)} p95_ms=${p95.toFixed(3)}`,
);
if (p95 >= 100) {
  throw new Error(`RGBA p95 gate failed: ${p95.toFixed(3)}ms`);
}
