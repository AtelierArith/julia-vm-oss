#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

function usage() {
  console.error("Usage: wasm_fixture_runner.mjs <pkg-dir> <fixture-tsv> <fixtures-root> <allowlist-tsv>");
}

if (process.argv.length !== 6) {
  usage();
  process.exit(2);
}

const [pkgDir, fixtureTsv, fixturesRoot, allowlistTsv] = process.argv.slice(2);
const pkg = await import(path.resolve(pkgDir, "subset_julia_vm_web.js"));

function parseCases(tsvPath) {
  const lines = fs.readFileSync(tsvPath, "utf8").split(/\r?\n/);
  const cases = [];
  for (const line of lines) {
    if (!line.trim() || line.startsWith("name\t")) continue;
    const [name, fixture, expected] = line.split("\t");
    if (!name || !fixture || !expected) {
      throw new Error(`malformed fixture smoke row: ${line}`);
    }
    cases.push({ name, fixture, expected });
  }
  return cases;
}

function parseAllowlist(tsvPath) {
  const lines = fs.readFileSync(tsvPath, "utf8").split(/\r?\n/);
  const allowlist = new Map();
  for (const line of lines) {
    if (!line.trim() || line.startsWith("name\t")) continue;
    const [name, fixture, issue, reason] = line.split("\t");
    if (!name || !fixture || !issue || !reason) {
      throw new Error(`malformed fixture smoke allowlist row: ${line}`);
    }
    allowlist.set(`${name}\t${fixture}`, { name, fixture, issue, reason, seen: false });
  }
  return allowlist;
}

let failures = 0;
let staleAllowlist = 0;
const allowlist = parseAllowlist(allowlistTsv);
for (const testCase of parseCases(fixtureTsv)) {
  const sourcePath = path.resolve(fixturesRoot, testCase.fixture);
  const source = fs.readFileSync(sourcePath, "utf8");
  const result = pkg.run_from_source(source, 0n);
  const typedValue = result.typed_value instanceof Map
    ? Object.fromEntries(result.typed_value)
    : result.typed_value;
  const actual = result.success && typedValue?.type === "bool"
    ? String(typedValue.value)
    : result.success
      ? JSON.stringify(typedValue)
      : `ERROR: ${result.error_message}`;
  const key = `${testCase.name}\t${testCase.fixture}`;
  const allowed = allowlist.get(key);
  const ok = result.success && actual === testCase.expected;
  if (allowed) {
    allowed.seen = true;
  }
  const status = ok ? "ok" : allowed ? "xfail" : "fail";
  console.log(`${testCase.name}\t${testCase.fixture}\t${status}\t${actual}`);
  if (!ok && !allowed) failures += 1;
  if (ok && allowed) {
    console.error(`STALE wasm fixture smoke allowlist entry now passes: ${key} (${allowed.issue})`);
    staleAllowlist += 1;
  }
}

for (const [key, entry] of allowlist.entries()) {
  if (!entry.seen) {
    console.error(`STALE wasm fixture smoke allowlist entry not selected: ${key} (${entry.issue})`);
    staleAllowlist += 1;
  }
}

process.exit(failures === 0 && staleAllowlist === 0 ? 0 : 1);
