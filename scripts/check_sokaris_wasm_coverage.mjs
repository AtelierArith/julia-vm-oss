#!/usr/bin/env node
// allow: SIZE_OK — this requested dependency-free audit must ship as one executable gate.

import { createHash } from "node:crypto";
import { lstat, readFile, readdir, stat } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const DEFAULT_MANIFEST = join(REPO_ROOT, "tests/sokaris/coverage.json");
const SOKARIS_ROOT = resolve(REPO_ROOT, "../sokaris");
const MODULES = new Map([
  ["Glyph", {
    file: "src/Glyph.jl",
    exports: ["⊙", "☽", "∅", "▷", "✧", "🝡", "⚕", "☿", "⚹", "✦", "☥", "⚸", "⇉", "𓇬"],
  }],
  ["Imhotep", {
    file: "src/Imhotep.jl",
    exports: [
      "invert", "gamma", "brightness", "contrast", "saturate", "desaturate", "grayscale",
      "gaussian", "box_blur", "median_blur", "motion_blur", "sharpen", "edge_detect", "emboss",
      "posterize", "threshold", "solarize", "noise", "pixelate", "crop", "crop_center", "crop_to",
      "scale_crop", "text_overlay", "glow", "load", "save",
    ],
  }],
]);
const HOST_SYMBOLS = new Set(["load", "save", "text_overlay"]);
const STATUSES = new Set(["planned", "red", "green"]);
const EFFECTS = new Set(["compute", "host"]);
const COMPARISONS = new Set(["exact", "absolute", "relative", "nan-aware"]);
const FORBIDDEN_NAMES = [
  "Sokaris", "Glyph", "Imhotep",
  "⊙", "☽", "∅", "▷", "✧", "🝡", "⚕", "☿", "⚹", "✦", "☥", "⚸", "⇉", "𓇬",
  "invert", "gamma", "brightness", "contrast", "saturate", "desaturate", "grayscale",
  "gaussian", "box_blur", "median_blur", "motion_blur", "sharpen", "edge_detect", "emboss",
  "posterize", "threshold", "solarize", "noise", "pixelate", "crop", "crop_center", "crop_to",
  "scale_crop", "text_overlay", "glow", "load", "save",
];
const DEFAULT_PRODUCTION_SCOPES = [
  "subset_julia_vm/src/aot",
  "subset_julia_vm_compile/src",
  "subset_julia_vm_lowering/src",
  "subset_julia_vm_web/src/compiler_api.rs",
  "subset_julia_vm_web/src/compiler_api",
  "subset_julia_vm/src/bin/aot.rs",
];
const MAX_MANIFEST_BYTES = 5 * 1024 * 1024;
const MAX_RUST_FILE_BYTES = 4 * 1024 * 1024;
const MAX_RUST_FILES = 10_000;
const MAX_DIRECTORY_DEPTH = 64;

function fail(message) {
  throw new Error(message);
}

function sha256(text) {
  return createHash("sha256").update(text).digest("hex");
}

function asObject(value, context) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail(`${context} must be an object`);
  }
  return value;
}

function nonEmptyString(value, context) {
  if (typeof value !== "string" || value.trim() === "") {
    fail(`${context} must be a non-empty string`);
  }
}

function parseArguments(argv) {
  const options = { manifest: DEFAULT_MANIFEST, productionRoot: null };
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    const value = argv[index + 1];
    if (option !== "--manifest" && option !== "--production-root") {
      fail(`unknown argument '${option}'; expected --manifest or --production-root`);
    }
    if (value === undefined || value.startsWith("--")) {
      fail(`${option} requires a path`);
    }
    const path = isAbsolute(value) ? value : resolve(process.cwd(), value);
    if (option === "--manifest") options.manifest = path;
    if (option === "--production-root") options.productionRoot = path;
    index += 1;
  }
  return options;
}

async function readText(path, context) {
  try {
    return await readFile(path, "utf8");
  } catch (error) {
    fail(`${context} is unreadable at ${path}: ${error.message}`);
  }
}

async function readJson(path) {
  const metadata = await stat(path).catch((error) => fail(`coverage manifest is unreadable at ${path}: ${error.message}`));
  if (metadata.size > MAX_MANIFEST_BYTES) fail(`coverage manifest exceeds ${MAX_MANIFEST_BYTES} bytes`);
  const text = await readText(path, "coverage manifest");
  try {
    return JSON.parse(text);
  } catch (error) {
    fail(`coverage manifest is not valid JSON: ${error.message}`);
  }
}

function parseExports(source, moduleName) {
  const exports = [];
  for (const [index, line] of source.split(/\r?\n/u).entries()) {
    const match = line.match(/^\s*export\s+(.+?)\s*$/u);
    if (!match) continue;
    for (const rawSymbol of match[1].split(",")) {
      const symbol = rawSymbol.trim();
      if (symbol === "") fail(`${moduleName}:${index + 1} has an empty export entry`);
      exports.push(symbol);
    }
  }
  return exports;
}

function validateUnique(values, context) {
  const seen = new Set();
  for (const value of values) {
    if (seen.has(value)) fail(`duplicate ${context}: '${value}'`);
    seen.add(value);
  }
}

function validateDescriptor(descriptor, context) {
  asObject(descriptor, context);
  nonEmptyString(descriptor.type, `${context}.type`);
  nonEmptyString(descriptor.elementType, `${context}.elementType`);
  if (!Number.isSafeInteger(descriptor.rank) || descriptor.rank < 0) {
    fail(`${context}.rank must be a non-negative safe integer`);
  }
}

function validateRowShape(row, index) {
  const context = `rows[${index}]`;
  asObject(row, context);
  nonEmptyString(row.module, `${context}.module`);
  nonEmptyString(row.symbol, `${context}.symbol`);
  nonEmptyString(row.caseId, `${context}.caseId`);
  nonEmptyString(row.fixture, `${context}.fixture`);
  nonEmptyString(row.case, `${context}.case`);
  nonEmptyString(row.exportWrapper, `${context}.exportWrapper`);
  nonEmptyString(row.wave, `${context}.wave`);
  if (!EFFECTS.has(row.effect)) fail(`${context}.effect must be compute or host`);
  if (!STATUSES.has(row.status)) fail(`${context}.status must be planned, red, or green`);

  const source = asObject(row.source, `${context}.source`);
  nonEmptyString(source.file, `${context}.source.file`);
  nonEmptyString(source.sha256, `${context}.source.sha256`);
  if (!/^[a-f0-9]{64}$/u.test(source.sha256)) fail(`${context}.source.sha256 must be lowercase SHA-256`);
  if (!Number.isSafeInteger(source.startLine) || source.startLine < 1) fail(`${context}.source.startLine must be positive`);
  if (!Number.isSafeInteger(source.endLine) || source.endLine < source.startLine) fail(`${context}.source.endLine must not precede startLine`);

  if (!Array.isArray(row.arguments)) fail(`${context}.arguments must be an array`);
  row.arguments.forEach((descriptor, argumentIndex) => validateDescriptor(descriptor, `${context}.arguments[${argumentIndex}]`));
  validateDescriptor(row.result, `${context}.result`);

  const comparison = asObject(row.comparison, `${context}.comparison`);
  if (!COMPARISONS.has(comparison.mode)) fail(`${context}.comparison.mode is unsupported`);
  if (typeof comparison.tolerance !== "number" || !Number.isFinite(comparison.tolerance) || comparison.tolerance < 0) {
    fail(`${context}.comparison.tolerance must be a finite non-negative number`);
  }
  if (comparison.mode === "exact" && comparison.tolerance !== 0) fail(`${context} exact comparison requires zero tolerance`);

  if (!Array.isArray(row.requiredImports)) fail(`${context}.requiredImports must be an array`);
  row.requiredImports.forEach((entry, importIndex) => {
    asObject(entry, `${context}.requiredImports[${importIndex}]`);
    nonEmptyString(entry.module, `${context}.requiredImports[${importIndex}].module`);
    nonEmptyString(entry.name, `${context}.requiredImports[${importIndex}].name`);
  });
}

async function loadSourceOracle(manifest) {
  const sourceFiles = asObject(manifest.sourceFiles, "sourceFiles");
  const oracle = new Map();
  for (const [moduleName, expected] of MODULES) {
    const expectedCount = expected.exports.length;
    const metadata = asObject(sourceFiles[moduleName], `sourceFiles.${moduleName}`);
    if (metadata.file !== expected.file) fail(`sourceFiles.${moduleName}.file must be '${expected.file}'`);
    if (metadata.expectedExports !== expectedCount) fail(`sourceFiles.${moduleName}.expectedExports must be ${expectedCount}`);
    nonEmptyString(metadata.sha256, `sourceFiles.${moduleName}.sha256`);
    const path = join(SOKARIS_ROOT, expected.file);
    const source = await readText(path, `${moduleName} source oracle`);
    const actualHash = sha256(source);
    if (metadata.sha256 !== actualHash) {
      fail(`${moduleName} full-source hash is stale: manifest ${metadata.sha256}, actual ${actualHash}`);
    }
    const exports = parseExports(source, moduleName);
    validateUnique(exports, `${moduleName} source export`);
    if (exports.length !== expectedCount) {
      fail(`${moduleName} source declares ${exports.length} exports; expected exactly ${expectedCount}`);
    }
    const missing = expected.exports.filter((symbol) => !exports.includes(symbol));
    const extras = exports.filter((symbol) => !expected.exports.includes(symbol));
    if (missing.length > 0 || extras.length > 0) {
      fail(`${moduleName} source export set drifted; missing [${missing.join(", ")}], extra [${extras.join(", ")}]`);
    }
    oracle.set(moduleName, { exports, lines: source.split(/\r?\n/u) });
  }
  const extraModules = Object.keys(sourceFiles).filter((name) => !MODULES.has(name));
  if (extraModules.length > 0) fail(`sourceFiles contains unexpected modules: ${extraModules.join(", ")}`);
  return oracle;
}

function validateRows(manifest, oracle) {
  if (manifest.schemaVersion !== 1) fail("schemaVersion must be exactly 1");
  if (!Array.isArray(manifest.rows)) fail("rows must be an array");
  if (manifest.rows.length !== 41) fail(`coverage manifest has ${manifest.rows.length}/41 rows`);
  manifest.rows.forEach(validateRowShape);
  validateUnique(manifest.rows.map((row) => `${row.module}.${row.symbol}`), "module/symbol row");
  validateUnique(manifest.rows.map((row) => row.caseId), "caseId");
  validateUnique(manifest.rows.map((row) => row.exportWrapper), "exportWrapper");

  for (const [moduleName, source] of oracle) {
    const rows = manifest.rows.filter((row) => row.module === moduleName);
    const expected = MODULES.get(moduleName).exports.length;
    if (rows.length !== expected) fail(`${moduleName} has ${rows.length}/${expected} manifest rows`);
    const rowSymbols = new Set(rows.map((row) => row.symbol));
    const missing = source.exports.filter((symbol) => !rowSymbols.has(symbol));
    const extras = rows.map((row) => row.symbol).filter((symbol) => !source.exports.includes(symbol));
    if (missing.length > 0) fail(`${moduleName} manifest omits source exports: ${missing.join(", ")}`);
    if (extras.length > 0) fail(`${moduleName} manifest adds non-exports: ${extras.join(", ")}`);
  }
  const unknownModules = manifest.rows.filter((row) => !MODULES.has(row.module));
  if (unknownModules.length > 0) fail(`manifest row has unknown module '${unknownModules[0].module}'`);

  for (const [index, row] of manifest.rows.entries()) {
    const moduleSource = oracle.get(row.module);
    const expectedFile = MODULES.get(row.module).file;
    if (row.source.file !== expectedFile) fail(`rows[${index}] source file must be '${expectedFile}'`);
    if (row.source.endLine > moduleSource.lines.length) fail(`rows[${index}] source span exceeds ${expectedFile}`);
    const span = moduleSource.lines.slice(row.source.startLine - 1, row.source.endLine).join("\n");
    const actualHash = sha256(span);
    if (actualHash !== row.source.sha256) {
      fail(`${row.module}.${row.symbol} source span/hash is stale at ${row.source.file}:${row.source.startLine}-${row.source.endLine}; actual ${actualHash}`);
    }
    if (!span.includes(row.symbol)) fail(`${row.module}.${row.symbol} source span does not contain its symbol`);
    const declaration = span.trimStart();
    const isImportedHost = (row.symbol === "load" || row.symbol === "save")
      && declaration.startsWith("using FileIO:");
    if (!isImportedHost && !declaration.startsWith(`${row.symbol}(`) && !declaration.startsWith(`function ${row.symbol}(`)) {
      fail(`${row.module}.${row.symbol} source span must start at its function declaration`);
    }

    const host = HOST_SYMBOLS.has(row.symbol);
    if (host && row.effect !== "host") fail(`${row.module}.${row.symbol} must declare host effect`);
    if (!host && row.effect !== "compute") fail(`${row.module}.${row.symbol} must declare compute effect`);
    if (host) {
      if (row.requiredImports.length !== 1 || row.requiredImports[0].module !== "sjulia_host" || row.requiredImports[0].name !== row.symbol) {
        fail(`${row.module}.${row.symbol} must declare exactly sjulia_host.${row.symbol}`);
      }
    } else if (row.requiredImports.length !== 0) {
      fail(`${row.module}.${row.symbol} is compute-only and must be import-free`);
    }
  }
  const hostCount = manifest.rows.filter((row) => row.effect === "host").length;
  const computeCount = manifest.rows.filter((row) => row.effect === "compute").length;
  if (hostCount !== 3 || computeCount !== 38) fail(`effect partition is ${computeCount} compute/${hostCount} host; expected 38/3`);
}

async function listRustFiles(path, depth = 0) {
  if (depth > MAX_DIRECTORY_DEPTH) fail(`production Rust audit exceeds directory depth ${MAX_DIRECTORY_DEPTH} at ${path}`);
  let metadata;
  try {
    metadata = await lstat(path);
  } catch (error) {
    fail(`production Rust audit scope is missing at ${path}: ${error.message}`);
  }
  if (metadata.isSymbolicLink()) fail(`production Rust audit refuses symbolic links: ${path}`);
  if (metadata.isFile()) return path.endsWith(".rs") ? [path] : [];
  const files = [];
  for (const entry of await readdir(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isSymbolicLink()) fail(`production Rust audit refuses symbolic links: ${child}`);
    if (entry.isDirectory()) files.push(...await listRustFiles(child, depth + 1));
    if (entry.isFile() && entry.name.endsWith(".rs")) files.push(child);
    if (files.length > MAX_RUST_FILES) fail(`production Rust audit exceeds ${MAX_RUST_FILES} files`);
  }
  return files;
}

function stripComments(source) {
  let result = "";
  let blockDepth = 0;
  let inString = false;
  let inChar = false;
  let rawHashes = null;
  let escaped = false;
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1];
    if (blockDepth > 0) {
      if (char === "/" && next === "*") { blockDepth += 1; result += "  "; index += 1; }
      else if (char === "*" && next === "/") { blockDepth -= 1; result += "  "; index += 1; }
      else result += char === "\n" ? "\n" : " ";
      continue;
    }
    if (rawHashes !== null) {
      const terminator = `"${"#".repeat(rawHashes)}`;
      if (source.startsWith(terminator, index)) {
        result += terminator;
        index += terminator.length - 1;
        rawHashes = null;
      } else result += char;
      continue;
    }
    if (!inString && !inChar) {
      const raw = source.slice(index).match(/^(?:br|rb|r)(#*)"/u);
      if (raw) {
        rawHashes = raw[1].length;
        result += raw[0];
        index += raw[0].length - 1;
        continue;
      }
    }
    if (!inString && !inChar && char === "/" && next === "*") { blockDepth = 1; result += "  "; index += 1; continue; }
    if (!inString && !inChar && char === "/" && next === "/") {
      while (index < source.length && source[index] !== "\n") { result += " "; index += 1; }
      result += "\n";
      continue;
    }
    result += char;
    if ((inString || inChar) && char === "\\" && !escaped) { escaped = true; continue; }
    if (!inChar && char === '"' && !escaped) inString = !inString;
    if (!inString && char === "'" && !escaped) inChar = !inChar;
    escaped = false;
  }
  return result;
}

function rustStringValues(source) {
  const values = [];
  for (let index = 0; index < source.length; index += 1) {
    const raw = source.slice(index).match(/^(?:br|rb|r)(#*)"/u);
    if (raw) {
      const terminator = `"${raw[1]}`;
      const start = index + raw[0].length;
      const end = source.indexOf(terminator, start);
      if (end < 0) break;
      values.push(source.slice(start, end));
      index = end + terminator.length - 1;
      continue;
    }
    const prefixLength = source.startsWith('b"', index) ? 1 : 0;
    if (source[index + prefixLength] !== '"') continue;
    let value = "";
    let escaped = false;
    index += prefixLength + 1;
    for (; index < source.length; index += 1) {
      const char = source[index];
      if (!escaped && char === '"') break;
      if (!escaped && char === "\\") { escaped = true; continue; }
      value += char;
      escaped = false;
    }
    values.push(value);
  }
  return values;
}

function removeTestModules(source) {
  const lines = source.split("\n");
  let skipping = false;
  let bodyStarted = false;
  let depth = 0;
  return lines.map((line) => {
    if (!skipping && /#\s*\[\s*(?:cfg\s*\(\s*test\s*\)|test)\s*\]/u.test(line)) {
      skipping = true;
      bodyStarted = false;
      depth = 0;
      return "";
    }
    if (!skipping) return line;
    const opens = (line.match(/\{/gu) ?? []).length;
    depth += opens;
    depth -= (line.match(/\}/gu) ?? []).length;
    bodyStarted ||= opens > 0;
    if ((bodyStarted && depth <= 0) || (!bodyStarted && line.includes(";"))) skipping = false;
    return "";
  }).join("\n");
}

function auditRustSource(path, source) {
  if (/(?:^|\/)tests?(?:\/|$)|_tests?\.rs$/u.test(path)) return [];
  const cleaned = removeTestModules(stripComments(source));
  const lines = cleaned.split("\n");
  const violations = [];
  const forbiddenConstants = new Map();
  for (const line of lines) {
    const declaration = line.match(/\b(?:const|static)\s+([A-Z][A-Z0-9_]*)\b[^=]*=(.*);/u);
    if (!declaration) continue;
    const values = rustStringValues(declaration[2]);
    for (const name of FORBIDDEN_NAMES) {
      if (values.includes(name)) forbiddenConstants.set(declaration[1], name);
    }
  }
  let matchDepth = null;
  let braceDepth = 0;
  let pendingCondition = "";
  for (const [index, line] of lines.entries()) {
    const trimmed = line.trim();
    const startsControl = /\b(?:if|else\s+if|while|match)\b|\bmatches!\s*\(/u.test(trimmed);
    if (startsControl) pendingCondition = `${pendingCondition} ${trimmed}`.trim();
    else if (pendingCondition !== "" && !/[{;]/u.test(pendingCondition)) pendingCondition += ` ${trimmed}`;

    const opens = (line.match(/\{/gu) ?? []).length;
    const closes = (line.match(/\}/gu) ?? []).length;
    if (/\bmatch\b/u.test(trimmed) && line.includes("{")) matchDepth = braceDepth + opens - closes;
    const inMatchArm = matchDepth !== null && braceDepth >= matchDepth && line.includes("=>");
    const directLogic = startsControl || inMatchArm || /(?:==|!=|\.contains\s*\(|\.starts_with\s*\(|\.ends_with\s*\()/.test(trimmed);
    const context = directLogic ? `${pendingCondition} ${trimmed}` : pendingCondition;

    const stringValues = rustStringValues(context);
    for (const name of FORBIDDEN_NAMES) {
      if ((directLogic || pendingCondition !== "") && stringValues.includes(name)) {
        violations.push(`${path}:${index + 1}: forbidden name '${name}' in matching/control-flow logic: ${trimmed}`);
      }
    }
    for (const [constant, name] of forbiddenConstants) {
      if ((directLogic || pendingCondition !== "") && new RegExp(`\\b${constant}\\b`, "u").test(context)) {
        violations.push(`${path}:${index + 1}: forbidden name '${name}' reaches matching/control-flow logic through ${constant}: ${trimmed}`);
      }
    }
    if (pendingCondition !== "" && /[{;]/u.test(trimmed)) pendingCondition = "";
    braceDepth += opens - closes;
    if (matchDepth !== null && braceDepth < matchDepth) matchDepth = null;
  }
  return violations;
}

async function auditProductionRust(productionRoot) {
  const scopes = productionRoot === null
    ? DEFAULT_PRODUCTION_SCOPES.map((scope) => join(REPO_ROOT, scope))
    : [productionRoot];
  const files = new Set();
  for (const scope of scopes) for (const file of await listRustFiles(scope)) files.add(file);
  if (files.size === 0) fail("production Rust audit found no .rs files; refusing to pass an empty scope");
  const violations = [];
  for (const file of [...files].sort()) {
    const metadata = await stat(file);
    if (metadata.size > MAX_RUST_FILE_BYTES) fail(`production Rust file exceeds ${MAX_RUST_FILE_BYTES} bytes: ${file}`);
    const source = await readText(file, "production Rust source");
    const display = productionRoot === null ? relative(REPO_ROOT, file) : relative(productionRoot, file);
    violations.push(...auditRustSource(display, source));
  }
  if (violations.length > 0) {
    fail(`production Rust contains Sokaris/package/export name shortcuts:\n  ${violations.join("\n  ")}`);
  }
  return files.size;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const manifest = asObject(await readJson(options.manifest), "coverage manifest");
  const oracle = await loadSourceOracle(manifest);
  validateRows(manifest, oracle);
  const rustFiles = await auditProductionRust(options.productionRoot);
  console.log("Glyph: 14/14 exports covered");
  console.log("Imhotep: 27/27 exports covered");
  console.log("Sokaris: 41/41 exports covered (38 compute, 3 sjulia_host)");
  console.log(`Production Rust shortcut audit: ${rustFiles} files checked`);
}

main().catch((error) => {
  console.error(`FAIL: ${error.message}`);
  process.exitCode = 1;
});
