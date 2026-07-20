#!/usr/bin/env node
// tools/lezer-oracle.mjs — Canonical CST oracle generator (Issue #11049, M0).
//
// Parses Julia source with the lezer-julia reference grammar
// (extern/lezer-julia; build it with `npm install` there) and emits Canonical
// CST JSON conforming to subset_julia_vm_parser_common/schemas/
// canonical-cst.schema.json (spec: Issue #11225):
//
//   {
//     "version": 1,
//     "root": { "kind": "SourceFile", "span": {"start": 0, "end": 42},
//               "children": [...], "value": null, "flags": [] },
//     "diagnostics": [...]
//   }
//
// Canonicalization (spec 05_canonical_cst.md §5.6; prototype normalizer —
// the Rust side, subset_julia_vm_parser_common, must stay in sync):
//   - Spans are UTF-8 BYTE offsets (start inclusive, end exclusive). lezer
//     reports UTF-16 code-unit offsets; this tool converts.
//   - Anonymous literal tokens ("(", "end", ...) are dropped; only named
//     nodes (uppercase first letter) and error nodes are kept.
//   - lezer node names are mapped to canonical NodeKind: Program→SourceFile,
//     ⚠→ErrorNode, all operator-token kinds (AssignmentOp, PlusOp, ...)→
//     Operator, BoolLiteral→BooleanLiteral, CharLiteral→CharacterLiteral,
//     StringLiteral→StringExpression, InterpExpression→Interpolation,
//     ArrowFunctionExpression→LambdaExpression, MacrocallExpression→
//     MacroCallExpression, BeginStatement→BeginBlock, LetStatement→
//     LetExpression, Generator→GeneratorExpression, GenFor→ForClause,
//     GenFilter→IfClause. Unmapped lezer names pass through verbatim.
//   - Inside string/command literals, uncovered text runs between the
//     delimiter tokens and interpolations are synthesized as StringFragment
//     leaves (lezer leaves fragment text as anonymous content).
//   - Leaf nodes carry `value`: {"Identifier": text} / {"Operator": text} /
//     {"IntegerLiteral": text} / {"FloatLiteral": text} /
//     {"CharacterLiteral": text} / {"StringFragment": text} /
//     {"Keyword": text} (BooleanLiteral and other keyword-shaped leaves);
//     other leaves carry value null.
//   - Every ErrorNode also produces an UNEXPECTED_TOKEN diagnostic.
//
// Usage:
//   node tools/lezer-oracle.mjs input.jl
//   node tools/lezer-oracle.mjs --stdin < input.jl
// Options:
//   --pretty              indent the JSON output
//   --canonical-only      (default) emit only the canonical document
//   --include-lezer-tree  add a "lezerTree" field with the raw lezer tree
//   --output FILE         write to FILE instead of stdout

import fs from "node:fs";
import process from "node:process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const lezerDist = path.join(repoRoot, "extern", "lezer-julia", "dist", "index.js");

if (!fs.existsSync(lezerDist)) {
  console.error(
    "ERROR: extern/lezer-julia/dist/index.js not found. Build the oracle first:\n" +
      "  bash scripts/populate_extern.sh lezer-julia && (cd extern/lezer-julia && npm install)"
  );
  process.exit(2);
}

const { parser } = await import(lezerDist);

// ---------------------------------------------------------------------------
// Canonicalization tables

const KIND_MAP = new Map([
  ["Program", "SourceFile"],
  ["BoolLiteral", "BooleanLiteral"],
  ["CharLiteral", "CharacterLiteral"],
  ["StringLiteral", "StringExpression"],
  ["NsStringLiteral", "NsStringExpression"],
  ["CommandLiteral", "CommandExpression"],
  ["NsCommandLiteral", "NsCommandExpression"],
  ["InterpExpression", "Interpolation"],
  ["ArrowFunctionExpression", "LambdaExpression"],
  ["MacrocallExpression", "MacroCallExpression"],
  ["BeginStatement", "BeginBlock"],
  ["LetStatement", "LetExpression"],
  ["Generator", "GeneratorExpression"],
  ["GenFor", "ForClause"],
  ["GenFilter", "IfClause"],
]);

// Operator-token node kinds all canonicalize to `Operator`; the operator text
// is preserved in the node value.
const OPERATOR_KINDS = new Set([
  "Operator", "ArrowOp", "AssignmentOp", "BitshiftOp", "Colon", "ComparisonOp",
  "Dollar", "EllipsisOp", "LazyAndOp", "LazyOrOp", "PairOp", "PipeLeftOp",
  "PipeRightOp", "PlusOp", "PowerOp", "RationalOp", "SubTypeOp", "TildeOp",
  "TimesOp", "TypeComparisonOp", "UnaryOp", "UnaryPlusOp", "UpdateOp",
]);

// String-shaped nodes whose uncovered interior runs become StringFragment.
const STRING_KINDS = new Set([
  "StringExpression", "NsStringExpression", "CommandExpression", "NsCommandExpression",
]);

// Keyword-shaped leaves: canonical kinds whose leaf value is Keyword(text).
const KEYWORD_VALUE_KINDS = new Set([
  "BooleanLiteral", "NothingLiteral", "BreakStatement", "ContinueStatement",
]);

const NAMED = /^[A-Z]/;

function leafValue(kind, text) {
  if (kind === "Identifier" || kind === "MacroIdentifier" || kind === "Symbol") {
    return { Identifier: text };
  }
  if (kind === "Operator") return { Operator: text };
  if (kind === "IntegerLiteral") return { IntegerLiteral: text };
  if (kind === "FloatLiteral") return { FloatLiteral: text };
  if (kind === "CharacterLiteral") return { CharacterLiteral: text };
  if (kind === "StringFragment" || kind === "EscapeSequence") return { StringFragment: text };
  if (KEYWORD_VALUE_KINDS.has(kind)) return { Keyword: text };
  return null;
}

// ---------------------------------------------------------------------------

// Map UTF-16 code-unit index -> UTF-8 byte offset for the whole source.
function utf16ToUtf8Map(src) {
  const map = new Uint32Array(src.length + 1);
  let bytes = 0;
  let i = 0;
  while (i < src.length) {
    map[i] = bytes;
    const cp = src.codePointAt(i);
    const units = cp > 0xffff ? 2 : 1;
    bytes += cp <= 0x7f ? 1 : cp <= 0x7ff ? 2 : cp <= 0xffff ? 3 : 4;
    if (units === 2) map[i + 1] = bytes; // never a node boundary; keep monotone
    i += units;
  }
  map[src.length] = bytes;
  return map;
}

export function canonicalDocument(src, { includeLezerTree = false } = {}) {
  const tree = parser.parse(src);
  const byteOf = utf16ToUtf8Map(src);
  const diagnostics = [];

  function span(from, to) {
    return { start: byteOf[from], end: byteOf[to] };
  }

  function fragment(from, to) {
    return {
      kind: "StringFragment",
      span: span(from, to),
      children: [],
      value: { StringFragment: src.slice(from, to) },
      flags: [],
    };
  }

  function convert(node) {
    const isError = node.type.isError;
    let kind = isError ? "ErrorNode" : (KIND_MAP.get(node.name) ?? node.name);
    if (OPERATOR_KINDS.has(node.name) && !isError) kind = "Operator";

    if (isError) {
      diagnostics.push({
        code: "UNEXPECTED_TOKEN",
        severity: "error",
        message:
          node.from === node.to
            ? "parser inserted a missing token during error recovery"
            : `unexpected input: ${JSON.stringify(src.slice(node.from, node.to))}`,
        span: span(node.from, node.to),
        expected: [],
        recovery: node.from === node.to ? "InsertedToken" : "SkippedToken",
      });
    }

    const children = [];
    const isString = STRING_KINDS.has(kind);
    // In string-shaped nodes, cursor tracks covered UTF-16 offsets so that
    // uncovered interior runs (fragment text lezer leaves anonymous) become
    // StringFragment leaves. Delimiters are the anonymous first/last tokens.
    let cursor = null;
    for (let child = node.firstChild; child; child = child.nextSibling) {
      const named = child.type.isError || NAMED.test(child.name);
      if (isString) {
        if (cursor !== null && child.from > cursor) children.push(fragment(cursor, child.from));
        if (!named && cursor === null) {
          // opening delimiter: fragments start after it
          cursor = child.to;
          continue;
        }
        cursor = child.to;
      }
      if (named) children.push(convert(child));
    }

    const text = src.slice(node.from, node.to);
    const value = children.length === 0 ? leafValue(kind, text) : null;
    return { kind, span: span(node.from, node.to), children, value, flags: [] };
  }

  const doc = { version: 1, root: convert(tree.topNode), diagnostics };
  if (includeLezerTree) doc.lezerTree = rawLezerTree(tree.topNode, src, byteOf);
  return doc;
}

function rawLezerTree(node, src, byteOf) {
  const children = [];
  for (let child = node.firstChild; child; child = child.nextSibling) {
    children.push(rawLezerTree(child, src, byteOf));
  }
  const out = { name: node.name, span: [byteOf[node.from], byteOf[node.to]] };
  if (children.length) out.children = children;
  else out.text = src.slice(node.from, node.to);
  return out;
}

function main() {
  const args = process.argv.slice(2);
  const opts = { pretty: false, includeLezerTree: false, output: null };
  let input = null;
  let useStdin = false;
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === "--pretty") opts.pretty = true;
    else if (a === "--canonical-only") opts.includeLezerTree = false;
    else if (a === "--include-lezer-tree") opts.includeLezerTree = true;
    else if (a === "--stdin") useStdin = true;
    else if (a === "--output") opts.output = args[++i];
    else if (a.startsWith("-")) {
      console.error(`unknown option: ${a}`);
      process.exit(2);
    } else input = a;
  }
  if (!useStdin && input === null) {
    console.error("usage: lezer-oracle.mjs [--pretty] [--include-lezer-tree] [--output FILE] <input.jl | --stdin>");
    process.exit(2);
  }
  const src = fs.readFileSync(useStdin ? 0 : input, "utf-8");
  const doc = canonicalDocument(src, opts);
  const json = JSON.stringify(doc, null, opts.pretty ? 2 : 0) + "\n";
  if (opts.output) fs.writeFileSync(opts.output, json);
  else process.stdout.write(json);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
