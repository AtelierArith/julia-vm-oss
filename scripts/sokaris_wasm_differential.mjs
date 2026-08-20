#!/usr/bin/env node
// allow: SIZE_OK — the executable orchestrator keeps event ordering and cleanup in one auditable state machine.

import { createHash, randomUUID } from "node:crypto";
import { spawn } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { buildSourceBundle, loadHarnessFixtures, SourceBundleError } from "../tests/sokaris/support/build_source_bundle.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const defaultCoveragePath = join(repoRoot, "tests/sokaris/coverage.json");
const checkerPath = join(repoRoot, "scripts/check_sokaris_wasm_coverage.mjs");
const oraclePath = join(repoRoot, "scripts/sokaris_julia_oracle.jl");
const nodeRunnerPath = join(repoRoot, "scripts/sokaris_node_runner.mjs");
const sokarisRoot = resolve(repoRoot, "../sokaris");
const compilerRoot = join(repoRoot, "pkg-compiler-final");
const artifactManifestPath = join(compilerRoot, "ARTIFACT_MANIFEST.json");
const compilerJavaScriptPath = join(compilerRoot, "subset_julia_vm_web.js");
const compilerWasmPath = join(compilerRoot, "subset_julia_vm_web_bg.wasm");
const defaultOutputRoot = join(repoRoot, "target/sokaris-parity");
const selectorNames = new Set(["--case", "--wave", "--module", "--all"]);
const compilerTimingNames = [
  "source_parse_lower_ms",
  "dead_code_elimination_ms",
  "type_inference_ms",
  "ir_conversion_ms",
  "optimization_ms",
  "wasm_ir_lowering_ms",
  "wasm_codegen_ms",
  "total_ms",
];

export class HarnessError extends Error {
  constructor(code, message, details = undefined) {
    super(message);
    this.name = "HarnessError";
    this.code = code;
    this.details = details;
  }
}

function parsePositiveTimeout(value, name, fallback) {
  if (value === undefined) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new HarnessError("invalid_configuration", `${name} must be a positive integer`);
  return parsed;
}

export function parseArguments(argv) {
  const options = { selector: null, keepArtifacts: false, requireUpstream: true };
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "--keep-artifacts") {
      options.keepArtifacts = true;
      continue;
    }
    if (option === "--require-upstream") continue;
    if (!selectorNames.has(option)) throw new HarnessError("unknown_selector", `unknown argument '${option}'`);
    if (options.selector !== null) throw new HarnessError("contradictory_selector", "choose exactly one of --case, --wave, --module, or --all");
    if (option === "--all") {
      options.selector = { kind: "all", value: null };
      continue;
    }
    const value = argv[index + 1];
    if (value === undefined || value.startsWith("--")) throw new HarnessError("invalid_selector", `${option} requires a value`);
    options.selector = { kind: option.slice(2), value };
    index += 1;
  }
  if (options.selector === null) throw new HarnessError("missing_selector", "choose one of --case, --wave, --module, or --all");
  return options;
}

export function selectRows(rows, selector) {
  let selected;
  if (selector.kind === "all") selected = rows;
  else if (selector.kind === "case") selected = rows.filter((row) => row.caseId === selector.value);
  else if (selector.kind === "wave") selected = rows.filter((row) => row.wave === selector.value);
  else if (selector.kind === "module") selected = rows.filter((row) => row.module === selector.value);
  else throw new HarnessError("unknown_selector", `unknown selector kind '${selector.kind}'`);
  if (selected.length === 0) throw new HarnessError("zero_cases_selected", `${selector.kind} selector '${selector.value}' matched zero cases`);
  return selected;
}

function normalizeFloat(value) {
  if (value === "NaN") return Number.NaN;
  if (value === "Infinity") return Infinity;
  if (value === "-Infinity") return -Infinity;
  return value;
}

export function compareTypedValues(expected, actual, comparison) {
  if (expected?.kind !== actual?.kind) {
    throw new HarnessError("result_type_mismatch", `expected ${expected?.kind}, received ${actual?.kind}`);
  }
  const mode = comparison?.mode;
  const tolerance = comparison?.tolerance;
  if (!Number.isFinite(tolerance) || tolerance < 0) throw new HarnessError("invalid_tolerance", "comparison tolerance must be finite and non-negative");
  if (mode === "exact") {
    if (tolerance !== 0) throw new HarnessError("invalid_tolerance", "exact comparison requires zero tolerance");
    if (!Object.is(expected.value, actual.value)) throw new HarnessError("tolerance_mismatch", "exact values differ", { expected, actual });
    return;
  }
  if (!["absolute", "relative", "nan-aware"].includes(mode)) throw new HarnessError("unsupported_comparison", `unsupported comparison mode '${mode}'`);
  const left = normalizeFloat(expected.value);
  const right = normalizeFloat(actual.value);
  if (typeof left !== "number" || typeof right !== "number") throw new HarnessError("result_type_mismatch", `${mode} comparison requires numeric values`);
  if (mode === "nan-aware" && Number.isNaN(left) && Number.isNaN(right)) return;
  if (!Number.isFinite(left) || !Number.isFinite(right)) {
    if (Object.is(left, right)) return;
    throw new HarnessError("tolerance_mismatch", "non-finite values differ", { expected, actual });
  }
  const absoluteError = Math.abs(left - right);
  const allowed = mode === "relative" ? tolerance * Math.max(Math.abs(left), Math.abs(right)) : tolerance;
  if (absoluteError > allowed) throw new HarnessError("tolerance_mismatch", `${mode} error ${absoluteError} exceeds ${allowed}`, { expected, actual });
}

function parseSingleJson(stdout, code, context) {
  const value = stdout.trim();
  if (value === "") throw new HarnessError(code, `${context} returned empty output`);
  try {
    return JSON.parse(value);
  } catch (error) {
    throw new HarnessError(code, `${context} returned malformed JSON: ${error.message}`, { stdout: value });
  }
}

export async function runChild(command, args, { cwd, env = process.env, timeoutMs, signal } = {}) {
  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(command, args, { cwd, env, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    let settled = false;
    const finish = (callback) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      signal?.removeEventListener("abort", abort);
      callback();
    };
    const abort = () => {
      child.kill("SIGKILL");
      finish(() => rejectPromise(new HarnessError("child_interrupted", `${command} was interrupted`)));
    };
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      finish(() => rejectPromise(new HarnessError("child_timeout", `${command} exceeded ${timeoutMs}ms`)));
    }, timeoutMs);
    signal?.addEventListener("abort", abort, { once: true });
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", (error) => finish(() => rejectPromise(new HarnessError("child_spawn_failure", `cannot execute ${command}: ${error.message}`))));
    child.on("close", (code, childSignal) => finish(() => resolvePromise({ code, signal: childSignal, stdout, stderr })));
  });
}

async function validateCoverage(runChildImpl, timeoutMs, signal) {
  const coveragePath = process.env.SOKARIS_COVERAGE_MANIFEST ?? defaultCoveragePath;
  const result = await runChildImpl(process.execPath, [checkerPath, "--manifest", coveragePath], { cwd: repoRoot, timeoutMs, signal });
  if (result.code !== 0) throw new HarnessError("coverage_contract_stale", "coverage contract validation failed", { stderr: result.stderr, stdout: result.stdout });
}

async function expectedJuliaVersion() {
  const manifest = await readFile(join(sokarisRoot, "Manifest.toml"), "utf8");
  const match = manifest.match(/^julia_version\s*=\s*"([^"]+)"/mu);
  if (match === null) throw new HarnessError("julia_version_requirement_missing", "Sokaris Manifest.toml does not pin julia_version");
  return process.env.SOKARIS_EXPECTED_JULIA_VERSION ?? match[1];
}

async function runJulia(row, fixture, runChildImpl, timeoutMs, signal) {
  const julia = process.env.SOKARIS_JULIA ?? "julia";
  const script = process.env.SOKARIS_ORACLE_SCRIPT ?? oraclePath;
  let result;
  try {
    result = await runChildImpl(julia, [`--project=${sokarisRoot}`, "--startup-file=no", script], {
      cwd: repoRoot,
      timeoutMs,
      signal,
      env: {
        ...process.env,
        JULIA_PKG_OFFLINE: "true",
        SOKARIS_CASE_ID: row.caseId,
        SOKARIS_MODULE: row.module,
        SOKARIS_SYMBOL: row.symbol,
        SOKARIS_ORACLE_EXPRESSION: fixture?.oracleExpression ?? "",
        SOKARIS_EXPECTED_JULIA_VERSION: await expectedJuliaVersion(),
      },
    });
  } catch (error) {
    if (error.code === "child_spawn_failure") {
      throw new HarnessError("missing_julia_executable", `${julia} is unavailable; install the Julia version pinned by ../sokaris/Manifest.toml or set SOKARIS_JULIA`, error.details);
    }
    throw error;
  }
  let payload;
  try {
    payload = parseSingleJson(result.stdout, "malformed_oracle_json", "Julia oracle");
  } catch (error) {
    if (result.code !== 0 && result.stdout.trim() === "") {
      throw new HarnessError("julia_oracle_failed", `Julia oracle exited ${result.code}`, { stderr: result.stderr });
    }
    throw error;
  }
  if (result.code !== 0 || payload.status !== "passed") {
    throw new HarnessError(payload.code ?? "julia_oracle_failed", payload.message ?? `Julia oracle exited ${result.code}`, { stderr: result.stderr });
  }
  if (payload.schemaVersion !== 1 || payload.caseId !== row.caseId || payload.result === undefined) {
    throw new HarnessError("malformed_oracle_json", "Julia oracle payload does not match schema version 1 or selected case");
  }
  return payload;
}

async function validateCompilerArtifact() {
  const manifest = JSON.parse(await readFile(artifactManifestPath, "utf8"));
  if (manifest.compiler_abi_version !== 2) {
    throw new HarnessError("compiler_abi_mismatch", `artifact manifest pins ABI ${manifest.compiler_abi_version}; generated-module ABI 2 is required`);
  }
  if (typeof manifest.source_commit !== "string" || !/^[0-9a-f]{40}$/u.test(manifest.source_commit)) {
    throw new HarnessError("compiler_source_commit_invalid", "artifact manifest must pin the 40-character source commit used for the build");
  }
  let compilerBytes;
  let compilerSource;
  for (const [name, path] of [["subset_julia_vm_web.js", compilerJavaScriptPath], ["subset_julia_vm_web_bg.wasm", compilerWasmPath]]) {
    const expected = manifest.artifacts?.[name];
    const bytes = await readFile(path);
    const actual = createHash("sha256").update(bytes).digest("hex");
    if (expected?.sha256 !== actual || expected.bytes !== bytes.length) {
      throw new HarnessError("compiler_artifact_mismatch", `${name} does not match ARTIFACT_MANIFEST.json`, { expected, actual, bytes: bytes.length });
    }
    if (name.endsWith(".wasm")) compilerBytes = bytes;
    else compilerSource = bytes.toString("utf8");
  }
  return { manifest, bytes: compilerBytes, source: compilerSource };
}

async function compileBundle(row, fixture, bundle) {
  if (process.env.SOKARIS_COMPILER_TEST_MODE === "diagnostic") {
    return { success: false, diagnostics: [{ code: "test_compiler_diagnostic", kind: "unsupported", message: "injected compiler diagnostic" }] };
  }
  const artifact = await validateCompilerArtifact();
  const compiler = await import(`data:text/javascript;base64,${Buffer.from(artifact.source).toString("base64")}`);
  await compiler.default({ module_or_path: artifact.bytes });
  const result = compiler.compile_to_wasm(bundle, {
    source_name: `${row.caseId}.jl`,
    opt_level: 2,
    exports: [{ export_name: row.exportWrapper, function_name: fixture.compiler.functionName, arg_types: fixture.compiler.argTypes }],
  });
  if (result.abi_version !== artifact.manifest.compiler_abi_version) {
    throw new HarnessError("compiler_abi_mismatch", `compiler returned ABI ${result.abi_version}, artifact manifest pins ${artifact.manifest.compiler_abi_version}`);
  }
  if (result.success) {
    if (!(result.wasm_bytes instanceof Uint8Array) || !WebAssembly.validate(result.wasm_bytes)) {
      throw new HarnessError("compiler_invalid_wasm", "compiler reported success without a valid Uint8Array Wasm module");
    }
    if (!Array.isArray(result.diagnostics) || result.diagnostics.length !== 0) {
      throw new HarnessError("compiler_result_malformed", "compiler success must contain an empty diagnostics array");
    }
    for (const name of compilerTimingNames) {
      if (!Number.isFinite(result.phase_timings?.[name]) || result.phase_timings[name] < 0) {
        throw new HarnessError("compiler_result_malformed", `compiler timing '${name}' must be finite and non-negative`);
      }
    }
  } else if (!Array.isArray(result.diagnostics) || result.diagnostics.length === 0) {
    throw new HarnessError("compiler_result_malformed", "compiler failure must contain at least one typed diagnostic");
  }
  return result;
}

async function runNode(row, fixture, wasmPath, requestPath, runChildImpl, timeoutMs, signal) {
  const request = {
    schemaVersion: 1,
    wasmPath,
    exportName: row.exportWrapper,
    requiredImports: row.requiredImports,
    arguments: fixture.compiler.arguments,
    result: row.result,
  };
  await writeFile(requestPath, `${JSON.stringify(request, null, 2)}\n`);
  const executable = process.env.SOKARIS_NODE ?? process.execPath;
  const runner = process.env.SOKARIS_NODE_RUNNER ?? nodeRunnerPath;
  const result = await runChildImpl(executable, [runner, requestPath], { cwd: repoRoot, timeoutMs, signal });
  const payload = parseSingleJson(result.stdout, "malformed_node_result", "Node runner");
  if (result.code !== 0 || payload.status !== "passed") {
    throw new HarnessError(payload.code ?? "node_runner_failed", payload.message ?? `Node runner exited ${result.code}`, payload.details);
  }
  if (payload.schemaVersion !== 1 || payload.result === undefined) throw new HarnessError("malformed_node_result", "Node result does not match schema version 1");
  return payload;
}

function errorDetails(error) {
  return { code: error.code ?? "harness_internal_error", message: error.message, details: error.details };
}

export async function runDifferential(argv, dependencies = {}) {
  const options = parseArguments(argv);
  const runChildImpl = dependencies.runChild ?? runChild;
  const outputRoot = dependencies.outputRoot ?? process.env.SOKARIS_OUTPUT_ROOT ?? defaultOutputRoot;
  const childTimeoutMs = parsePositiveTimeout(process.env.SOKARIS_CHILD_TIMEOUT_MS, "SOKARIS_CHILD_TIMEOUT_MS", 120_000);
  const runId = `${new Date().toISOString().replaceAll(/[:.]/gu, "-")}-${randomUUID()}`;
  const runDir = join(outputRoot, runId);
  const evidencePath = join(outputRoot, `${runId}.ndjson`);
  const events = [];
  let sequence = 0;
  const emit = async (caseId, event, fields = {}) => {
    const record = { schemaVersion: 1, caseId, event, sequence: ++sequence, timestamp: new Date().toISOString(), ...fields };
    events.push(record);
    if (dependencies.quiet !== true) process.stdout.write(`${JSON.stringify(record)}\n`);
    await writeFile(evidencePath, `${events.map((entry) => JSON.stringify(entry)).join("\n")}\n`);
  };
  const controller = new AbortController();
  const interrupt = () => controller.abort();
  process.once("SIGINT", interrupt);
  process.once("SIGTERM", interrupt);
  await mkdir(runDir, { recursive: true });
  let exitCode = 0;
  try {
    await validateCoverage(runChildImpl, childTimeoutMs, controller.signal);
    const coveragePath = process.env.SOKARIS_COVERAGE_MANIFEST ?? defaultCoveragePath;
    const coverage = JSON.parse(await readFile(coveragePath, "utf8"));
    const selected = selectRows(coverage.rows, options.selector);
    for (const row of selected) {
      if (!/^[a-z0-9][a-z0-9_-]*$/u.test(row.caseId)) {
        throw new HarnessError("unsafe_case_id", `caseId '${row.caseId}' is not filesystem-safe`);
      }
    }
    const fixtures = await loadHarnessFixtures();
    for (const row of selected) {
      const fixture = fixtures[row.caseId];
      const caseStarted = performance.now();
      await emit(row.caseId, "julia_started", { module: row.module, wave: row.wave });
      let oracle;
      try {
        oracle = await runJulia(row, fixture, runChildImpl, childTimeoutMs, controller.signal);
      } catch (error) {
        await emit(row.caseId, "julia_failed", { ...errorDetails(error), durationMs: performance.now() - caseStarted, finalStatus: "failed" });
        exitCode = 1;
        continue;
      }
      await emit(row.caseId, "julia_passed", { juliaVersion: oracle.juliaVersion, durationMs: performance.now() - caseStarted });
      await emit(row.caseId, "compile_started");
      const compileStarted = performance.now();
      let sourceBundle;
      let compiled;
      try {
        sourceBundle = await buildSourceBundle({ row, fixture });
        compiled = await compileBundle(row, fixture, sourceBundle.bundle);
        if (!compiled.success) {
          const expectedPlanned = fixture?.evidenceKind === "planned_unsupported";
          await emit(row.caseId, "compile_failed", {
            code: "compiler_diagnostic",
            diagnostics: compiled.diagnostics,
            compilerTimingsMs: compiled.phase_timings,
            durationMs: performance.now() - compileStarted,
            finalStatus: expectedPlanned ? "planned_compiler_diagnostic" : "failed",
          });
          if (!expectedPlanned) exitCode = 1;
          continue;
        }
      } catch (error) {
        const planned = error instanceof SourceBundleError && error.code === "fixture_not_implemented";
        await emit(row.caseId, "compile_failed", { ...errorDetails(error), durationMs: performance.now() - compileStarted, finalStatus: planned ? "planned_compiler_diagnostic" : "failed" });
        if (!planned) exitCode = 1;
        continue;
      }
      const caseDir = join(runDir, row.caseId);
      await mkdir(caseDir, { recursive: true });
      const sourcePath = join(caseDir, "bundle.jl");
      const wasmPath = join(caseDir, "module.wasm");
      const requestPath = join(caseDir, "node-request.json");
      await writeFile(sourcePath, sourceBundle.bundle);
      await writeFile(wasmPath, compiled.wasm_bytes);
      await emit(row.caseId, "compile_passed", {
        compilerVersion: compiled.compiler_version,
        abiVersion: compiled.abi_version,
        compilerTimingsMs: compiled.phase_timings,
        sourceSpanSha256: sourceBundle.sourceSpanSha256,
        durationMs: performance.now() - compileStarted,
      });
      const nodeStarted = performance.now();
      let nodeResult;
      try {
        nodeResult = await runNode(row, fixture, wasmPath, requestPath, runChildImpl, childTimeoutMs, controller.signal);
      } catch (error) {
        await emit(row.caseId, "node_failed", { ...errorDetails(error), durationMs: performance.now() - nodeStarted, finalStatus: "failed" });
        exitCode = 1;
        continue;
      }
      await emit(row.caseId, "node_passed", { result: nodeResult.result, durationMs: performance.now() - nodeStarted });
      try {
        compareTypedValues(oracle.result, nodeResult.result, row.comparison);
        await emit(row.caseId, "compare_passed", {
          comparison: row.comparison,
          evidenceKind: fixture.evidenceKind,
          durationMs: performance.now() - caseStarted,
          finalStatus: fixture.evidenceKind === "harness_mechanics" ? "mechanics_passed" : "parity_passed",
        });
      } catch (error) {
        await emit(row.caseId, "compare_failed", { ...errorDetails(error), comparison: row.comparison, finalStatus: "failed" });
        exitCode = 1;
      }
    }
  } finally {
    process.removeListener("SIGINT", interrupt);
    process.removeListener("SIGTERM", interrupt);
    if (!options.keepArtifacts) await rm(runDir, { recursive: true, force: true });
  }
  return { exitCode, events, evidencePath, runDir };
}

async function main() {
  try {
    const result = await runDifferential(process.argv.slice(2));
    process.exitCode = result.exitCode;
  } catch (error) {
    const record = { schemaVersion: 1, caseId: null, event: "harness_failed", sequence: 1, timestamp: new Date().toISOString(), ...errorDetails(error), finalStatus: "failed" };
    console.log(JSON.stringify(record));
    process.exitCode = 1;
  }
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) await main();
