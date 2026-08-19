#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);
const packageUrl = new URL("pkg-compiler-final/", root);
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
  return WebAssembly.instantiate(module, {});
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
const descriptor = 32;
const pointer = 128;
const input = new Uint8Array(memory.buffer, pointer, 4);
const descriptorView = new DataView(memory.buffer);
input.set([1, 2, 254, 0]);
descriptorView.setUint32(descriptor, 2, true);
descriptorView.setUint32(descriptor + 4, 0, true);
descriptorView.setUint32(descriptor + 8, 1, true);
descriptorView.setUint32(descriptor + 12, 1, true);
descriptorView.setUint32(descriptor + 16, 0, true);
descriptorView.setUint32(descriptor + 20, 1, true);
descriptorView.setUint32(descriptor + 24, pointer, true);
descriptorView.setUint32(descriptor + 28, 0, true);
descriptorView.setBigUint64(descriptor + 32, BigInt(input.length), true);
descriptorView.setBigUint64(descriptor + 40, BigInt(input.length), true);
descriptorView.setBigInt64(descriptor + 48, 1n, true);
assert.equal(mutationInstance.exports["increment!"](descriptor), 4n);
assert.deepEqual(Array.from(input), [2, 3, 255, 1]);
assert.equal(mutationInstance.exports.__sjulia_wasm_abi_version(), mutation.abi_version);

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

const unsupported = compiler.compile_to_wasm(
  "string_identity(value::String)::String = value",
  {
    exports: [
      {
        export_name: "string_identity",
        function_name: "string_identity",
        arg_types: ["String"],
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
