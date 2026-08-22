#!/usr/bin/env node
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

import { createSokarisHostImports } from "./sokaris_host_imports.mjs";

export class NodeRunnerError extends Error {
  constructor(code, message, details = undefined) {
    super(message);
    this.name = "NodeRunnerError";
    this.code = code;
    this.details = details;
  }
}

function typedArgument(argument) {
  if (argument?.kind === "f64") return Number(argument.value);
  if (argument?.kind === "i64") return BigInt(argument.value);
  if (argument?.kind === "bool") return argument.value ? 1 : 0;
  if (argument?.kind === "u8" && Number.isInteger(argument.value) && argument.value >= 0 && argument.value <= 255) {
    return argument.value;
  }
  throw new NodeRunnerError("malformed_argument", `unsupported argument encoding '${argument?.kind}'`);
}

export function decodeResult(value, descriptor) {
  if (descriptor === null || typeof descriptor !== "object" || !Number.isInteger(descriptor.rank)) {
    throw new NodeRunnerError("malformed_result_descriptor", "result descriptor is malformed");
  }
  if (descriptor.rank !== 0) {
    throw new NodeRunnerError("unsupported_result_shape", `rank-${descriptor.rank} result decoding is deferred until generated arrays can be returned`);
  }
  if (descriptor.elementType === "f64") {
    if (typeof value !== "number") throw new NodeRunnerError("malformed_result", "expected a JavaScript number for f64");
    return { kind: "f64", value: Number.isNaN(value) ? "NaN" : value === Infinity ? "Infinity" : value === -Infinity ? "-Infinity" : value };
  }
  if (descriptor.elementType === "i64") {
    if (typeof value !== "bigint") throw new NodeRunnerError("malformed_result", "expected a JavaScript bigint for i64");
    return { kind: "i64", value: value.toString() };
  }
  if (descriptor.elementType === "bool") {
    if (value !== 0 && value !== 1) throw new NodeRunnerError("malformed_result", "expected 0 or 1 for Bool");
    return { kind: "bool", value: value === 1 };
  }
  if (descriptor.elementType === "u8") {
    if (!Number.isInteger(value) || value < 0 || value > 255) throw new NodeRunnerError("malformed_result", "expected an unsigned byte");
    return { kind: "u8", value };
  }
  if (descriptor.elementType === "none") {
    if (value !== undefined) throw new NodeRunnerError("malformed_result", "expected no Wasm result");
    return { kind: "none", value: null };
  }
  throw new NodeRunnerError("unsupported_result_shape", `element type '${descriptor.elementType}' is not decodable by the current result adapter`);
}

function importIdentity(entry) {
  return `${entry.module}.${entry.name}`;
}

export async function runNodeRequest(request, dependencies = {}) {
  const readBytes = dependencies.readBytes ?? ((path) => readFile(path));
  const compile = dependencies.compile ?? ((bytes) => WebAssembly.compile(bytes));
  const instantiate = dependencies.instantiate ?? ((module, imports) => WebAssembly.instantiate(module, imports));
  const moduleImports = dependencies.moduleImports ?? ((module) => WebAssembly.Module.imports(module));
  let module;
  try {
    module = await compile(await readBytes(request.wasmPath));
  } catch (error) {
    throw new NodeRunnerError("node_validation_failure", `generated Wasm did not validate: ${error.message}`);
  }
  const actualImports = moduleImports(module).map(importIdentity).sort();
  const requiredImports = (request.requiredImports ?? []).map(importIdentity).sort();
  if (JSON.stringify(actualImports) !== JSON.stringify(requiredImports)) {
    throw new NodeRunnerError("node_import_mismatch", "generated Wasm imports do not match the coverage manifest", { actualImports, requiredImports });
  }
  let instanceExports;
  const hostFactory = dependencies.hostFactory ?? ((options) => createSokarisHostImports(options));
  const hostDependencies = dependencies.hostDependencies ?? {
    loadImage: () => { throw new NodeRunnerError("host_import_not_configured", "loadImage is not configured"); },
    saveImage: () => { throw new NodeRunnerError("host_import_not_configured", "saveImage is not configured"); },
    renderText: () => { throw new NodeRunnerError("host_import_not_configured", "renderText is not configured"); },
  };
  const imports = actualImports.length === 0
    ? {}
    : hostFactory({
        memory: { get buffer() { return instanceExports?.memory?.buffer ?? new ArrayBuffer(0); } },
        allocate: (...args) => instanceExports.__sjulia_alloc(...args),
        ...hostDependencies,
      });
  let instance;
  try {
    instance = await instantiate(module, imports);
  } catch (error) {
    throw new NodeRunnerError("node_instantiation_failure", `generated Wasm could not instantiate: ${error.message}`);
  }
  instanceExports = instance.exports;
  const target = instance.exports[request.exportName];
  if (typeof target !== "function") throw new NodeRunnerError("missing_wasm_export", `missing export '${request.exportName}'`);
  let rawResult;
  try {
    rawResult = target(...(request.arguments ?? []).map(typedArgument));
  } catch (error) {
    throw new NodeRunnerError("node_trap", `generated Wasm trapped: ${error.message}`);
  }
  return { schemaVersion: 1, status: "passed", result: decodeResult(rawResult, request.result) };
}

async function main() {
  const requestPath = process.argv[2];
  if (requestPath === undefined || process.argv.length !== 3) {
    throw new NodeRunnerError("invalid_runner_arguments", "usage: sokaris_node_runner.mjs <request.json>");
  }
  const request = JSON.parse(await readFile(requestPath, "utf8"));
  return runNodeRequest(request);
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().then(
    (result) => console.log(JSON.stringify(result)),
    (error) => {
      console.log(JSON.stringify({ schemaVersion: 1, status: "failed", code: error.code ?? "node_runner_failure", message: error.message, details: error.details }));
      process.exitCode = 1;
    },
  );
}
