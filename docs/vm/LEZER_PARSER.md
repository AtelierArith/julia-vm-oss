# Lezer-compatible Pure Rust Parser (Issue #11049)

Status and operational guide for the lezer-compatible parser rewrite. The
normative specification is archived verbatim in Issue #11225 (originally
`extern/LezerCompatibleParserSpec/`, 16 docs + JSON schema + templates); the
JSON schema is committed at
`subset_julia_vm_parser_common/schemas/canonical-cst.schema.json`. This file tracks what is implemented in-tree and
how to run the tooling. Grammar-authority order is unchanged and documented
in `PARSER_GRAMMAR_REFERENCES.md`: upstream `julia/src/julia-parser.scm` is
normative, lezer-julia is the structural cross-check / differential oracle.

## Roadmap position

| Milestone (spec `15_roadmap.md`) | State |
|---|---|
| M0 Research Prototype — oracle CLI, canonical normalizer, corpus | **done** (this document's tooling) |
| M1 Common Model — Span/NodeKind/NodeValue/Diagnostic/serializer | **done** (`subset_julia_vm_parser_common`) |
| M2 Lexer | not started |
| M3 Expression Parser | not started |
| M4 Statement Parser | not started |
| M5 Lowering Adapter | not started |
| M6+ Differential CI / shadow mode / switch | not started |

## Components

### `subset_julia_vm_parser_common` (crate)

The Canonical CST common model (spec `05_canonical_cst.md`, `08_workspace_api.md`):

- `Span` — UTF-8 byte offsets, `start` inclusive / `end` exclusive.
- `NodeKind` — the canonical catalog (spec §5.4) plus an `Other(String)`
  passthrough for names the normalizer does not map yet; serialized as a
  plain JSON string.
- `NodeValue` — leaf payloads (`{"Identifier": "x"}`, `{"Operator": "+"}`, …).
- `CstNode` / `CanonicalDocument` — schema-conformant
  (`subset_julia_vm_parser_common/schemas/canonical-cst.schema.json`,
  `version: 1`).
- `Diagnostic` / `DiagnosticCode` / `Severity` — stable codes (spec §9.2).
- `CstNode::validate_spans` — the span invariants of spec §10.4.
- `CstNode::first_divergence` — first-mismatch tree diff for differential
  test output.

### `tools/lezer-oracle.mjs` (oracle CLI, spec §6.1)

Parses Julia source with lezer-julia and emits a Canonical CST document.
Development-only: Node.js is never a product dependency (spec G1).

```bash
node tools/lezer-oracle.mjs input.jl
node tools/lezer-oracle.mjs --stdin --pretty [--include-lezer-tree] [--output FILE]
```

Prerequisite (also recorded in `extern/MANIFEST.tsv`):

```bash
bash scripts/populate_extern.sh lezer-julia
(cd extern/lezer-julia && npm install)   # npm install also builds dist/
```

Canonicalization policy implemented by the normalizer (must stay in sync
with the Rust side; the full rule list is in the header of
`tools/lezer-oracle.mjs`):

- spans converted from lezer's UTF-16 code units to UTF-8 bytes;
- anonymous tokens dropped, named nodes kept;
- name mapping `Program→SourceFile`, `⚠→ErrorNode`, `BoolLiteral→
  BooleanLiteral`, `CharLiteral→CharacterLiteral`, `StringLiteral→
  StringExpression`, `InterpExpression→Interpolation`,
  `ArrowFunctionExpression→LambdaExpression`, `MacrocallExpression→
  MacroCallExpression`, `BeginStatement→BeginBlock`, `LetStatement→
  LetExpression`, `Generator→GeneratorExpression`, `GenFor→ForClause`,
  `GenFilter→IfClause`, and every operator-token kind (`AssignmentOp`,
  `PlusOp`, …) → `Operator`;
- uncovered text runs inside string/command literals synthesized as
  `StringFragment` leaves;
- one `UNEXPECTED_TOKEN` diagnostic per lezer error-recovery node
  (`InsertedToken` for zero-length, `SkippedToken` otherwise).

Unmapped lezer names (e.g. `Arguments`, `Signature`) currently pass through
verbatim and deserialize as `NodeKind::Other`; they will be pinned down as
M3/M4 fix the tree shapes that contain them.

### Oracle snapshots (differential corpus seed, spec §10.1)

`subset_julia_vm_parser_common/tests/oracle_snapshots/*.json` — one file per
lezer-julia `test/*.txt` corpus file (130 cases), each case
`{name, source, document}`. Committed so the Rust tests run without Node.js.
Regenerate only when `extern/lezer-julia` is updated, and update
`extern/MANIFEST.tsv` in the same PR (oracle version pinning, spec §6.5):

```bash
bash scripts/gen_lezer_oracle_snapshots.sh
```

Validated by `subset_julia_vm_parser_common/tests/oracle_snapshot_tests.rs`:
schema deserialization, span invariants, and a canary that canonicalized
lezer names never leak into snapshots.

## Next steps

- M2 lexer in a new `subset_julia_vm_parser_lezer` crate (spec `04_lexer_and_parser.md` §4.1).
- `subset_julia_vm_parser_diff` corpus runner comparing legacy / new / oracle
  (spec §6.3 difference classification).
- Legacy parser output adapter to Canonical CST so the three-way diff can run.
