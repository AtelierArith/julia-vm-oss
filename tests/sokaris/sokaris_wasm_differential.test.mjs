import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  compareTypedValues,
  parseArguments,
  runChild,
  selectRows,
} from "../../scripts/sokaris_wasm_differential.mjs";
import { decodeResult, NodeRunnerError, runNodeRequest } from "../../scripts/sokaris_node_runner.mjs";
import { buildSourceBundle } from "./support/build_source_bundle.mjs";

const repoRoot = new URL("../../", import.meta.url);
const harness = new URL("scripts/sokaris_wasm_differential.mjs", repoRoot);
const originalEnvironment = { ...process.env };

afterEach(() => {
  for (const key of Object.keys(process.env)) if (!(key in originalEnvironment)) delete process.env[key];
  Object.assign(process.env, originalEnvironment);
});

function runHarness(args, environment = {}) {
  return spawnSync(process.execPath, [harness.pathname, ...args], {
    cwd: repoRoot,
    encoding: "utf8",
    timeout: 150_000,
    env: { ...process.env, ...environment },
  });
}

function parseEvents(stdout) {
  return stdout.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
}

describe("Sokaris Wasm differential harness", { concurrency: false }, () => {
test("glyph-apply records Julia completion before compiler work", () => {
  const run = runHarness(["--case", "glyph-apply", "--require-upstream"]);

  assert.equal(run.status, 0, run.stderr);
  const events = parseEvents(run.stdout);
  const names = events.map((event) => event.event);
  assert.ok(names.indexOf("julia_started") < names.indexOf("julia_passed"));
  assert.ok(names.indexOf("julia_passed") < names.indexOf("compile_started"));
  assert.ok(names.includes("node_passed"));
  assert.equal(events.at(-1).finalStatus, "mechanics_passed");
});

test("selectors reject contradictions, unknown values, and zero cases", () => {
  assert.deepEqual(parseArguments(["--all"]), { selector: { kind: "all", value: null }, keepArtifacts: false, requireUpstream: true });
  assert.deepEqual(parseArguments(["--wave", "glyph"]), { selector: { kind: "wave", value: "glyph" }, keepArtifacts: false, requireUpstream: true });
  assert.deepEqual(parseArguments(["--module", "Glyph"]), { selector: { kind: "module", value: "Glyph" }, keepArtifacts: false, requireUpstream: true });
  assert.throws(() => parseArguments(["--case", "x", "--all"]), { code: "contradictory_selector" });
  assert.throws(() => parseArguments(["--wat"]), { code: "unknown_selector" });
  assert.throws(() => selectRows([{ caseId: "known", wave: "glyph", module: "Glyph" }], { kind: "case", value: "missing" }), { code: "zero_cases_selected" });
});

test("source bundling rejects paths outside the canonical Sokaris root", async () => {
  await assert.rejects(buildSourceBundle({
    row: {
      caseId: "escape",
      source: { file: "../subset-julia-wasm-compiler/AGENTS.md", startLine: 1, endLine: 1, sha256: "0".repeat(64) },
    },
    fixture: { compiler: { source: "f() = 1" } },
  }), { code: "source_path_escape" });
});

test("comparison modes enforce type, shape, finite tolerance, and mismatches", () => {
  compareTypedValues({ kind: "i64", value: "42" }, { kind: "i64", value: "42" }, { mode: "exact", tolerance: 0 });
  compareTypedValues({ kind: "f64", value: 10 }, { kind: "f64", value: 10.1 }, { mode: "absolute", tolerance: 0.11 });
  compareTypedValues({ kind: "f64", value: 100 }, { kind: "f64", value: 101 }, { mode: "relative", tolerance: 0.01 });
  compareTypedValues({ kind: "f64", value: "NaN" }, { kind: "f64", value: "NaN" }, { mode: "nan-aware", tolerance: 0 });
  assert.throws(() => compareTypedValues({ kind: "i64", value: "1" }, { kind: "f64", value: 1 }, { mode: "exact", tolerance: 0 }), { code: "result_type_mismatch" });
  assert.throws(() => compareTypedValues({ kind: "f64", value: 1 }, { kind: "f64", value: 2 }, { mode: "absolute", tolerance: 0.5 }), { code: "tolerance_mismatch" });
  assert.throws(() => compareTypedValues({ kind: "f64", value: 1 }, { kind: "f64", value: 1 }, { mode: "absolute", tolerance: Infinity }), { code: "invalid_tolerance" });
});

test("result decoder rejects malformed and future descriptor shapes", () => {
  assert.deepEqual(decodeResult(42, { elementType: "f64", rank: 0 }), { kind: "f64", value: 42 });
  assert.throws(() => decodeResult("42", { elementType: "f64", rank: 0 }), { code: "malformed_result" });
  assert.throws(() => decodeResult(0, { elementType: "f64", rank: 2 }), { code: "unsupported_result_shape" });
  assert.throws(() => decodeResult(0, null), { code: "malformed_result_descriptor" });
});

test("Node runner types validation, import, instantiation, and trap failures", async () => {
  const request = { wasmPath: "unused", exportName: "f", requiredImports: [], arguments: [], result: { elementType: "f64", rank: 0 } };
  const base = { readBytes: async () => new Uint8Array(), moduleImports: () => [] };
  await assert.rejects(runNodeRequest(request, { ...base, compile: async () => { throw new Error("bad bytes"); } }), { code: "node_validation_failure" });
  await assert.rejects(runNodeRequest(request, { ...base, compile: async () => ({}), moduleImports: () => [{ module: "x", name: "y" }] }), { code: "node_import_mismatch" });
  await assert.rejects(runNodeRequest(request, { ...base, compile: async () => ({}), instantiate: async () => { throw new Error("link"); } }), { code: "node_instantiation_failure" });
  await assert.rejects(runNodeRequest(request, { ...base, compile: async () => ({}), instantiate: async () => ({ exports: { f: () => { throw new WebAssembly.RuntimeError("trap"); } } }) }), { code: "node_trap" });
});

test("Node runner wires declared Sokaris host imports", async () => {
  const request = {
    wasmPath: "unused",
    exportName: "f",
    requiredImports: [{ module: "sjulia_host", name: "load" }],
    arguments: [],
    result: { elementType: "i64", rank: 0 },
  };
  const memory = new WebAssembly.Memory({ initial: 1 });
  let linkedImports;
  const result = await runNodeRequest(request, {
    readBytes: async () => new Uint8Array(),
    compile: async () => ({}),
    moduleImports: () => [{ module: "sjulia_host", name: "load" }],
    hostFactory: ({ memory: lazyMemory, allocate }) => ({
      sjulia_host: {
        load: () => {
          assert.equal(lazyMemory.buffer, memory.buffer);
          assert.equal(allocate(1n, 1), 64);
          return 0;
        },
      },
    }),
    instantiate: async (_module, imports) => {
      linkedImports = imports;
      return {
        exports: {
          memory,
          __sjulia_alloc: () => 64,
          f: () => BigInt(imports.sjulia_host.load()),
        },
      };
    },
  });
  assert.equal(typeof linkedImports.sjulia_host.load, "function");
  assert.deepEqual(result.result, { kind: "i64", value: "0" });
});

test("missing Julia executable stops before compile", () => {
  const run = runHarness(["--case", "glyph-apply"], { SOKARIS_JULIA: "/definitely/missing/julia" });
  assert.equal(run.status, 1);
  const events = parseEvents(run.stdout);
  assert.equal(events.at(-1).code, "missing_julia_executable");
  assert.equal(events.some((event) => event.event === "compile_started"), false);
});

test("wrong Julia version stops before compile", () => {
  const run = runHarness(["--case", "glyph-apply"], { SOKARIS_EXPECTED_JULIA_VERSION: "0.0.0" });
  assert.equal(run.status, 1);
  const events = parseEvents(run.stdout);
  assert.equal(events.at(-1).code, "julia_version_mismatch");
  assert.equal(events.some((event) => event.event === "compile_started"), false);
});

test("oracle nonzero and malformed JSON never start compile", async (context) => {
  const directory = await mkdtemp(join(tmpdir(), "sokaris-oracle-negative-"));
  context.after(() => rm(directory, { recursive: true, force: true }));
  const failing = join(directory, "failing.jl");
  const malformed = join(directory, "malformed.jl");
  await writeFile(failing, "print(\"{\\\"schemaVersion\\\":1,\\\"caseId\\\":\\\"glyph-apply\\\",\\\"status\\\":\\\"passed\\\",\\\"result\\\":{\\\"kind\\\":\\\"f64\\\",\\\"value\\\":42}}\"); exit(7)\n");
  await writeFile(malformed, "print(\"not-json\")\n");
  for (const [script, code] of [[failing, "julia_oracle_failed"], [malformed, "malformed_oracle_json"]]) {
    const run = runHarness(["--case", "glyph-apply"], { SOKARIS_ORACLE_SCRIPT: script });
    assert.equal(run.status, 1);
    const events = parseEvents(run.stdout);
    assert.equal(events.at(-1).code, code);
    assert.equal(events.some((event) => event.event === "compile_started"), false);
  }
});

test("compiler diagnostics are typed after Julia passes", () => {
  const run = runHarness(["--case", "glyph-apply"], { SOKARIS_COMPILER_TEST_MODE: "diagnostic" });
  assert.equal(run.status, 1);
  const events = parseEvents(run.stdout);
  assert.deepEqual(events.map((event) => event.event), ["julia_started", "julia_passed", "compile_started", "compile_failed"]);
  assert.equal(events.at(-1).code, "compiler_diagnostic");
  assert.equal(events.at(-1).finalStatus, "failed");
});

test("stale coverage fails before case execution without tracked-file corruption", async (context) => {
  const directory = await mkdtemp(join(tmpdir(), "sokaris-coverage-negative-"));
  context.after(() => rm(directory, { recursive: true, force: true }));
  const manifest = JSON.parse(await readFile(new URL("coverage.json", import.meta.url), "utf8"));
  manifest.sourceFiles.Glyph.sha256 = "0".repeat(64);
  const path = join(directory, "coverage.json");
  await writeFile(path, JSON.stringify(manifest));
  const run = runHarness(["--case", "glyph-apply"], { SOKARIS_COVERAGE_MANIFEST: path });
  assert.equal(run.status, 1);
  const events = parseEvents(run.stdout);
  assert.equal(events.at(-1).code, "coverage_contract_stale");
  assert.equal(events.some((event) => event.event === "julia_started"), false);
});

test("artifact cleanup defaults on and --keep-artifacts preserves case files", async (context) => {
  const outputRoot = await mkdtemp(join(tmpdir(), "sokaris-artifacts-"));
  context.after(() => rm(outputRoot, { recursive: true, force: true }));
  const cleaned = runHarness(["--case", "glyph-apply"], { SOKARIS_OUTPUT_ROOT: outputRoot });
  assert.equal(cleaned.status, 0, cleaned.stderr);
  const cleanedEvidence = parseEvents(cleaned.stdout);
  assert.ok((await readdir(outputRoot)).some((name) => name.endsWith(".ndjson")));
  assert.equal(cleanedEvidence.at(-1).finalStatus, "mechanics_passed");

  const kept = runHarness(["--case", "glyph-apply", "--keep-artifacts"], { SOKARIS_OUTPUT_ROOT: outputRoot });
  assert.equal(kept.status, 0, kept.stderr);
  const entries = await readdir(outputRoot, { withFileTypes: true });
  const keptDirectory = entries.find((entry) => entry.isDirectory());
  assert.ok(keptDirectory);
  await stat(join(outputRoot, keptDirectory.name, "glyph-apply", "module.wasm"));
});

test("hung and interrupted children are killed with typed errors", async () => {
  await assert.rejects(runChild(process.execPath, ["-e", "setTimeout(() => {}, 10_000)"], { timeoutMs: 20 }), { code: "child_timeout" });
  const controller = new AbortController();
  const pending = runChild(process.execPath, ["-e", "setTimeout(() => {}, 10_000)"], { timeoutMs: 10_000, signal: controller.signal });
  controller.abort();
  await assert.rejects(pending, { code: "child_interrupted" });
});

test("NodeRunnerError remains a typed public failure", () => {
  const error = new NodeRunnerError("typed", "message");
  assert.equal(error.code, "typed");
});
});
