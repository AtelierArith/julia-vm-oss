#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);
const packageUrl = new URL("pkg-compiler-final/", root);
const artifactManifest = JSON.parse(
  await readFile(fileURLToPath(new URL("ARTIFACT_MANIFEST.json", packageUrl)), "utf8"),
);
assert.equal(
  artifactManifest.compiler_abi_version,
  2,
  "pkg-compiler-final must emit generated-module ABI v2",
);
const compiler = await import(new URL("subset_julia_vm_web.js", packageUrl));
const compilerBytes = await readFile(fileURLToPath(new URL("subset_julia_vm_web_bg.wasm", packageUrl)));
await compiler.default({ module_or_path: compilerBytes });

function compile(source, exportName, argTypes, functionName = exportName) {
  const result = compiler.compile_to_wasm(source, {
    source_name: `${exportName}.jl`,
    opt_level: 2,
    exports: [{ export_name: exportName, function_name: functionName, arg_types: argTypes }],
  });
  assert.equal(result.success, true, JSON.stringify(result.diagnostics));
  assert.ok(result.wasm_bytes instanceof Uint8Array);
  assert.ok(result.phase_timings.total_ms >= 0);
  assert.equal(WebAssembly.validate(result.wasm_bytes), true);
  return result;
}

async function instantiate(result) {
  const module = await WebAssembly.compile(result.wasm_bytes);
  assert.deepEqual(WebAssembly.Module.imports(module), []);
  const instance = await WebAssembly.instantiate(module, {});
  assert.equal(instance.exports.__sjulia_wasm_abi_version(), 2);
  for (const name of ["__sjulia_alloc", "__sjulia_free", "__sjulia_drop"]) {
    assert.equal(typeof instance.exports[name], "function", `missing generated export ${name}`);
  }
  return instance;
}

function assertTrap(action, message) {
  assert.throws(action, WebAssembly.RuntimeError, message);
}

function writeRankOneUint8Descriptor(memory, descriptor, pointer, length, flags = 0) {
  const view = new DataView(memory.buffer);
  view.setUint32(descriptor, 2, true);
  view.setUint32(descriptor + 4, flags, true);
  view.setUint32(descriptor + 8, 1, true);
  view.setUint32(descriptor + 12, 1, true);
  view.setUint32(descriptor + 16, 0, true);
  view.setUint32(descriptor + 20, 1, true);
  view.setUint32(descriptor + 24, pointer, true);
  view.setUint32(descriptor + 28, 0, true);
  view.setBigUint64(descriptor + 32, BigInt(length), true);
  view.setBigUint64(descriptor + 40, BigInt(length), true);
  view.setBigInt64(descriptor + 48, 1n, true);
}

const arithmetic = compile(
  "add_scale(x::Int64, y::Int64) = (x + y) * 2",
  "public_add_scale",
  ["Int64", "Int64"],
  "add_scale",
);
const arithmeticInstance = await instantiate(arithmetic);
assert.equal(arithmeticInstance.exports.public_add_scale(10n, 11n), 42n);
assert.equal(Object.hasOwn(arithmeticInstance.exports, "add_scale"), false);

const omittedOptions = compiler.compile_to_wasm("forty_two() = 42", undefined);
assert.equal(omittedOptions.success, true, JSON.stringify(omittedOptions.diagnostics));
assert.ok(omittedOptions.wasm_bytes instanceof Uint8Array);

const script = compiler.compile_to_wasm(
  `load(path::String)::Array{UInt8,3} = Array{UInt8,3}(undef, 0, 0, 0)
image = load("inputs/input.png")`,
  {
    entry_mode: "script",
    imports: [
      {
        module: "sjulia_host",
        name: "load",
        function_name: "load",
        params: ["String"],
        result: "Array{UInt8,3}",
      },
    ],
  },
);
assert.equal(script.success, true, JSON.stringify(script.diagnostics));
assert.equal(script.entry_point, "__sjulia_script_entry");
assert.deepEqual(script.imports, [
  {
    module: "sjulia_host",
    name: "load",
    function_name: "load",
    params: ["String"],
    result: "Array{UInt8, 3}",
  },
]);
const scriptModule = await WebAssembly.compile(script.wasm_bytes);
assert.deepEqual(WebAssembly.Module.imports(scriptModule), [
  { module: "sjulia_host", name: "load", kind: "function" },
]);
assert.ok(WebAssembly.Module.exports(scriptModule).some(({ name }) => name === script.entry_point));

const repeatedArithmetic = compile(
  "add_scale(x::Int64, y::Int64) = (x + y) * 2",
  "public_add_scale",
  ["Int64", "Int64"],
  "add_scale",
);
assert.deepEqual(repeatedArithmetic.wasm_bytes, arithmetic.wasm_bytes);

const mutation = compile(
  "function increment!(bytes::Vector{UInt8})\ni = 1\nwhile i <= length(bytes)\nbytes[i] = UInt8(bytes[i] + 1)\ni = i + 1\nend\nreturn length(bytes)\nend",
  "increment!",
  ["Vector{UInt8}"],
);
const mutationInstance = await instantiate(mutation);
const memory = mutationInstance.exports.memory;
const { __sjulia_alloc: allocate, __sjulia_free: free, __sjulia_drop: drop } = mutationInstance.exports;
const pointer = allocate(4n, 1);
assert.notEqual(pointer, 0);
let input = new Uint8Array(memory.buffer, pointer, 4);
input.set([1, 2, 254, 0]);
const descriptor = allocate(56n, 8);
assert.notEqual(descriptor, 0);
writeRankOneUint8Descriptor(memory, descriptor, pointer, input.length);
assert.equal(mutationInstance.exports["increment!"](descriptor), 4n);
input = new Uint8Array(memory.buffer, pointer, 4);
assert.deepEqual(Array.from(input), [2, 3, 255, 1]);
assert.equal(mutationInstance.exports.__sjulia_wasm_abi_version(), mutation.abi_version);

const malformed = new DataView(memory.buffer);
malformed.setUint32(descriptor, 1, true);
assertTrap(() => mutationInstance.exports["increment!"](descriptor), "ABI1 descriptors must trap");
writeRankOneUint8Descriptor(memory, descriptor, pointer, input.length);
malformed.setUint32(descriptor + 20, 2, true);
assertTrap(() => mutationInstance.exports["increment!"](descriptor), "rank mismatch must trap");
writeRankOneUint8Descriptor(memory, descriptor, pointer, input.length);
malformed.setUint32(descriptor + 28, 1, true);
assertTrap(() => mutationInstance.exports["increment!"](descriptor), "reserved fields must trap");
writeRankOneUint8Descriptor(memory, descriptor, pointer, input.length, 1);
drop(descriptor);
const dropped = new DataView(memory.buffer);
assert.equal(dropped.getUint32(descriptor + 4, true), 0);
assert.equal(dropped.getUint32(descriptor + 24, true), 0);
assertTrap(() => drop(descriptor), "repeated descriptor drop must trap");
free(descriptor);

const staleView = new Uint8Array(memory.buffer);
const growthAllocation = allocate(BigInt(memory.buffer.byteLength + 65_536), 8);
assert.notEqual(growthAllocation, 0);
assert.equal(staleView.byteLength, 0, "memory growth must detach stale JavaScript views");
const refreshedView = new Uint8Array(memory.buffer, growthAllocation, 1);
refreshedView[0] = 7;
assert.equal(refreshedView[0], 7);
free(growthAllocation);

const invalid = compiler.compile_to_wasm("x = 1\ny = )\n", undefined);
assert.equal(invalid.success, false);
assert.equal(invalid.diagnostics[0].kind, "parse");
assert.equal(invalid.diagnostics[0].span.start_line, 2);

const oversized = compiler.compile_to_wasm("x".repeat(1_048_577), undefined);
assert.equal(oversized.success, false);
assert.equal(oversized.diagnostics[0].code, "source_too_large");

const invalidOptions = compiler.compile_to_wasm("forty_two() = 42", { opt_level: "fast" });
assert.equal(invalidOptions.success, false);
assert.equal(invalidOptions.diagnostics[0].code, "invalid_options");

const invalidArgumentType = compiler.compile_to_wasm("identity(x) = x", {
  exports: [
    {
      export_name: "identity",
      function_name: "identity",
      arg_types: ["NotAJuliaType"],
    },
  ],
});
assert.equal(invalidArgumentType.success, false);
assert.equal(invalidArgumentType.diagnostics[0].code, "invalid_argument_type");

const stringIdentity = compile(
  "string_identity(value::String)::String = value",
  "string_identity",
  ["String"],
);
const stringInstance = await instantiate(stringIdentity);
const stringMemory = stringInstance.exports.memory;
const stringBytes = new TextEncoder().encode("héllo 🐱");
const stringPointer = stringInstance.exports.__sjulia_alloc(BigInt(stringBytes.length), 1);
new Uint8Array(stringMemory.buffer, stringPointer, stringBytes.length).set(stringBytes);
const stringView = stringInstance.exports.__sjulia_alloc(8n, 4);
const inputView = new DataView(stringMemory.buffer);
inputView.setUint32(stringView, stringPointer, true);
inputView.setUint32(stringView + 4, stringBytes.length, true);
const resultViewPointer = stringInstance.exports.string_identity(stringView);
const resultView = new DataView(stringMemory.buffer);
const resultPointer = resultView.getUint32(resultViewPointer, true);
const resultByteLength = resultView.getUint32(resultViewPointer + 4, true);
const resultText = new TextDecoder("utf-8", { fatal: true }).decode(
  new Uint8Array(stringMemory.buffer, resultPointer, resultByteLength),
);
assert.equal(resultText, "héllo 🐱");
assert.equal(resultByteLength, stringBytes.length);
stringInstance.exports.__sjulia_free(stringView);
stringInstance.exports.__sjulia_free(stringPointer);

const unsupported = compiler.compile_to_wasm(
  'interpolate(value::Int64)::String = "value = $value"',
  {
    exports: [
      {
        export_name: "interpolate",
        function_name: "interpolate",
        arg_types: ["Int64"],
      },
    ],
  },
);
assert.equal(unsupported.success, false);
assert.equal(unsupported.diagnostics[0].kind, "unsupported");

console.log(
  JSON.stringify({
    compiler_version: arithmetic.compiler_version,
    abi_version: arithmetic.abi_version,
    arithmetic_bytes: arithmetic.wasm_bytes.length,
    mutation_bytes: mutation.wasm_bytes.length,
    arithmetic_timings_ms: arithmetic.phase_timings,
    mutation_timings_ms: mutation.phase_timings,
  }),
);
