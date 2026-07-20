# Parser Grammar References — lezer-julia as a Secondary Cross-Check (Issue #10985)

## Authority order

1. **`julia/src/julia-parser.scm`** (upstream flisp parser, vendored under
   `./julia`) — the **normative** grammar authority for sjulia
   (`REPOSITORY_RULES.md` Design Principle 2, Upstream-Driven Compatibility).
   Every parser-behavior decision is verified against `julia --startup-file=no`.
2. **[JuliaPluto/lezer-julia](https://github.com/JuliaPluto/lezer-julia)**
   (`src/julia.grammar`) — a **secondary, structural cross-check**. It is a
   compact LR/GLR grammar for the same language (initially derived from
   tree-sitter-julia, maintained for Pluto's CodeMirror editor), useful because
   it makes *rule-shape* decisions explicit that are procedural and scattered
   in the flisp parser.

lezer-julia is NOT an authority: where it disagrees with `julia-parser.scm` or
observed `julia` behavior, upstream wins. Its value is as a readable map of
how a second independent implementation carved up the same grammar, and as a
source of adversarial test inputs (its `test/` corpus).

## Rule-shape correspondences (verified during Milestone 73)

The Milestone 73 parser fixes ("Parser, Lowering, and Syntax Compatibility
Gaps, 2026-07 follow-up") converged sjulia's parser onto the same structural
decisions lezer-julia encodes declaratively:

| Grammar area | lezer-julia shape | sjulia counterpart (Milestone 73) |
|---|---|---|
| Syntactic vs value operators | `SyntacticOperator[@name=Operator]` is a dedicated rule (`.`, `<:`-family, `...`, `&&`, `\|\|`, `?`, `->`, assignment/update ops) excluded from identifier positions; value operators (`+`, `*`, `\|>`, …) are ordinary `Operator` tokens | `Token::is_syntactic_operator()` single authority behind `is_operator_identifier` / `reject_invalid_operator_identifier` (Issues #10932/#10940, PR #11026); role-inventory table `corpus_operators.rs` mirrors the "each token has explicit roles" idea |
| `global`/`local` statements | `GlobalStatement`/`LocalStatement` wrap a following expression/declaration item list, not just bare identifiers | `parse_var_declaration_item` delegates statement keywords (`function`, `macro`, `module`, control flow) to their construct parsers, wrapped in Global/Local declaration nodes (Issues #10937/#10945, PR #11027), mirroring upstream's `(global local)` → `parse-eq` arm |
| Typed expressions `::` | `::` participates in the unary/binary typed grammar; it is never an operator value | `DoubleColon` removed from operator-identifier space; bare `::::` is a premature-end-of-input error, `:::: Int` parses as nested unary-typed (Issue #10915, PR #11026) |
| Type-parameter bounds | subtype comparisons in type-head position are comparison-chain shaped (`TypeHeadSubtype`, `TypeComparisonOp` = `<:`/`>:`); double bounds arise from chained comparison | `struct DB{Int8<:T<:Signed}` parses via the comparison-chain shape into `SubtypeConstraint [name, upper, lower]`, same node shape as the `where` form (Issue #10644, PR #11026) |
| Precedence | one explicit `@precedence` block, lowest→highest, with GLR ambiguity markers | sjulia keeps upstream's precedence tiers; the scoped-declaration RHS matrix (Issue #10951, PR #11027) pins the tiers (`=`, pair `=>`, arrow, ternary, comma tuples) against upstream behavior |

## How to use lezer-julia when extending the parser

- **Before adding a token role** (operator, identifier continuation,
  declaration modifier): check how `src/julia.grammar` classifies the token —
  if it appears in `SyntacticOperator` there, expect upstream to reject it as
  an identifier/value, and encode that in the role-inventory tables
  (`corpus_operators.rs`, Issue #10940; lexer boundary tables, Issue #10848).
- **When a flisp code path is hard to read**: find the corresponding lezer
  rule to form a hypothesis, then confirm against `julia --startup-file=no`
  probes before implementing. Never implement from lezer-julia alone.
- **Test inputs**: lezer-julia's `test/` corpus files are grammar-area-sorted
  and make good seeds for `subset_julia_vm_parser` corpus/malformed-input
  tests (compare expected CST shapes only against upstream `julia`, not
  against lezer's tree).

## Status

Adopted as the documented secondary reference (this file). The concrete
parser corrections that motivated Issue #10985 landed via the Milestone 73
PRs listed in the table above; future parser work should consult this file's
"How to use" section. (Issue #10985)
