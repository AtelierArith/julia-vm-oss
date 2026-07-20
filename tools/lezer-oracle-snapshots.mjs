#!/usr/bin/env node
// tools/lezer-oracle-snapshots.mjs — regenerate the checked-in Canonical CST
// oracle snapshots from the lezer-julia test corpus (Issue #11049, M0).
//
// Reads every extern/lezer-julia/test/*.txt corpus file (lezer fileTests
// format), converts each case with tools/lezer-oracle.mjs's canonicalizer,
// and writes one JSON snapshot per corpus file to
// subset_julia_vm_parser_common/tests/oracle_snapshots/<name>.json:
//
//   [ { "name": "...", "source": "...", "document": { version, root, diagnostics } }, ... ]
//
// The snapshots are committed so the Rust differential tests run without
// Node.js. Rerun via scripts/gen_lezer_oracle_snapshots.sh after updating
// extern/lezer-julia (and update extern/MANIFEST.tsv in the same PR).

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { canonicalDocument } from "./lezer-oracle.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const testDir = path.join(repoRoot, "extern", "lezer-julia", "test");
const outDir = path.join(repoRoot, "subset_julia_vm_parser_common", "tests", "oracle_snapshots");

// Resolve @lezer/generator from extern/lezer-julia's node_modules (this
// script lives outside that package, so bare specifiers don't resolve here).
const { fileTests } = await import(
  pathToFileURL(
    path.join(repoRoot, "extern", "lezer-julia", "node_modules", "@lezer", "generator", "dist", "test.js")
  )
);

fs.mkdirSync(outDir, { recursive: true });

let files = 0;
let cases = 0;
for (const entry of fs.readdirSync(testDir).sort()) {
  if (!entry.endsWith(".txt")) continue;
  const content = fs.readFileSync(path.join(testDir, entry), "utf-8");
  const tests = fileTests(content, entry);
  const snapshot = tests.map((t) => ({
    name: t.name,
    source: t.text,
    document: canonicalDocument(t.text),
  }));
  const outPath = path.join(outDir, entry.replace(/\.txt$/, ".json"));
  fs.writeFileSync(outPath, JSON.stringify(snapshot, null, 2) + "\n");
  files += 1;
  cases += snapshot.length;
  console.log(`${outPath}: ${snapshot.length} cases`);
}
console.log(`wrote ${files} snapshot files, ${cases} cases total`);
