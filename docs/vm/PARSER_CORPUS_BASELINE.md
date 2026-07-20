# Parser Corpus Differential Baseline

Issue #8614 / #8635. First recorded baseline of sweeping the upstream
`julia/` submodule corpus through the sjulia parser (parse only — no
lowering, no VM execution).

## How to regenerate

```bash
git submodule update --init julia          # if the corpus is not checked out
bash scripts/parser_corpus_sweep.sh        # writes target/parser_corpus/sweep.tsv
```

The harness is the `parse_corpus` bin in `subset_julia_vm_parser`
(`subset_julia_vm_parser/src/bin/parse_corpus.rs`, shared logic in
`subset_julia_vm_parser/src/corpus.rs`). Each parse runs in a dedicated
large-stack thread with `catch_unwind`, so a parser panic is reported as a
`Panic` record instead of aborting the sweep. TSV columns:
`file`, `span` (`line:col-line:col`), `error_kind` (`ParseError` variant
name, `Panic`, or `ReadError`), `snippet` (source line at the span),
`message` (full error text). The sweep order is `LC_ALL=C` sorted, and the
output was verified byte-identical across runs (deterministic).

A file counts as **failing** when `parse_with_errors` reports at least one
error; error recovery can emit several (cascading) records per file, so
record counts overstate the number of distinct constructs.

## Baseline (2026-07-02)

- Corpus: `julia/` submodule at `2f3128cdb266784ef2928efa4f2560382d013d3f`
  (upstream `VERSION` 1.14.0-DEV), roots `base/`, `stdlib/`, `test/`,
  regular files matching `*.jl` only.

| Metric | Value |
|--------|-------|
| Files swept | 730 |
| Parsed cleanly | 464 (63.56%) |
| Files with parse errors | 266 |
| Parser panics | **0** |
| Divergence records (TSV rows) | 2,376 |

Per corpus root (files):

| Root | Files | Clean | Failing |
|------|-------|-------|---------|
| `julia/base` | 195 | 92 | 103 |
| `julia/stdlib` | 255 | 190 | 65 |
| `julia/test` | 280 | 182 | 98 |

Records by `error_kind`:

| error_kind | Records |
|------------|---------|
| `UnexpectedToken` | 2,268 |
| `LexerError` | 107 |
| `UnexpectedEof` | 1 |
| `Panic` | 0 |

Top failing files by record count (error recovery cascades inflate these;
`julia/test/syntax.jl` is intentionally pathological syntax):

| File | Records |
|------|---------|
| `julia/test/syntax.jl` | 248 |
| `julia/test/subtype.jl` | 120 |
| `julia/test/show.jl` | 114 |
| `julia/base/show.jl` | 107 |
| `julia/test/core.jl` | 94 |
| `julia/test/abstractarray.jl` | 80 |
| `julia/test/precompile.jl` | 73 |
| `julia/test/intfuncs.jl` | 68 |
| `julia/base/exports.jl` | 46 |
| `julia/base/loading.jl` | 42 |

## Notes

- **No panics**: the whole corpus parses without crashing the parser, so
  the #8635 "panics get individual `bug` Issues first" task has no open
  items at this baseline.
- `LexerError` records cluster around identifiers/operators outside the
  supported token set (e.g. `´`, `⟷`, emoji identifiers, `∛`/`∜`) — mostly
  in `julia/test` stress files.
- Classification of the divergences into (a) intentional subset exclusions
  (allowlist), (b) `unsupported-feature` Issues, (c) tree-shape `bug`
  Issues (JuliaSyntax.jl oracle) is tracked by #8636; the allowlist
  ratchet for CI is #8637.

## Incremental update (Issue #8759, 2026-07-03)

Fixes to the sjulia parser for additional corpus gaps (Issue #8759) reduced
parse errors in the most affected files:

| File | Records before | Records after | Notes |
|------|---------------|---------------|-------|
| `julia/base/show.jl` | 107 | **0** | Fully fixed; removed from allowlist |
| `julia/test/show.jl` | 114 | ~26 | Improved; remaining gaps tracked below |
| `julia/base/shell.jl` | 3 | 3 | Unchanged; `(@doc ... function...end)` pattern |

Changes that produced these improvements:

- `++` (`PlusPlus`) and `..` (`DotDot`) added to `is_operator()` so they can
  appear as quoted operator symbols (`:++`, `:.`).
- Unicode assignment operators `≔`/`⩴`/`≕` (`ColonEquals`, `DoubleColonEquals`,
  `EqualsColon`) given assignment-level `binary_precedence`.
- `RParen`/`RBracket`/`RBrace` added to `parse_return_statement` termination
  guard so `(expr; return)` parses correctly.
- `parse_colon_prefix`: compound dotted-assignment symbols `:(.\=)`, `:(.<<=)`,
  etc. produced by composing `DotOp` + `Eq` tokens; `:?` recognized as a symbol.
- Trailing comma before semicolon in `parse_parenthesized_or_tuple_inner`
  allows `(a=1, ; b=2)` named-tuple syntax.
- `parse_import_name`: parenthesized operator form `(..)` in import lists;
  `LParen` added to `is_import_name_start`.

Remaining `julia/test/show.jl` gaps (26 records, tagged for follow-up):
- `:(g(a,; b))` / `:(;)` — semicolon in quoted call args / empty block expr
- `:(∓ 1)` / `:(± 1)` — Unicode prefix operators (∓ lexed as identifier)
- `var"..."` — non-standard string identifiers (resolved by #8961)
- `:(import A.B: c.@d)` — dotted macro in import list
- `:(*{1, 2})` — curly parameterized expr with operator prefix
- `:(x for x in y for z in w)` — multi-clause generator inside quote
- `` :(`ls x y`) `` — backtick command literal inside quote
- `.&` / `a'ᵀ` — dotted binary operator as value, adjoint+superscript

## Update (2026-07-03) — implicit line continuation fix (Issue #8753)

Sweep on 673 files (submodule `15346901f0039751c5488744f1f62de7d87510a8`,
`VERSION` 1.14.0-DEV) after PR #8753 partial fix: implicit line continuation
inside `@macro(...)` and `:(...)` expressions. Before this fix, the pre-PR
sweep had 263 divergence records (633/673 files clean). After:

| Metric | Pre-fix | Post-fix | Delta |
|--------|---------|----------|-------|
| Files swept | 673 | 673 | — |
| Parsed cleanly | 633 (94.06%) | 639 (94.95%) | **+6** |
| Files with parse errors | 40 | 34 | **−6** |
| Parser panics | 0 | 0 | — |
| Divergence records | 263 | 235 | **−28** |

Contexts fixed by this PR:
- `@macro(\n args \n)`: parenthesized macro call with newlines before closing `)`;
  the parser now increments `grouping_depth` and skips newlines before `)`.
- `:(  \n  expr  \n)`: multi-line parenthesized quote expression; the parser
  now skips the initial newline and newlines before the closing `)`.

Files newly parsing cleanly (6): `julia/base/deprecated.jl`,
`julia/base/timing.jl`, `julia/stdlib/Unicode/test/runtests.jl`,
`julia/test/corelogging.jl`, `julia/test/deprecation_exec.jl`,
`julia/test/precompile_extmi.jl`.

Remaining `newline-continuation` family errors are cascade errors from
other root causes (e.g. semicolon call arguments, `@constprop`, statement
definitions in expression position, and `@UnionAll` braces) tracked under
#8753 for follow-up.

## Update (2026-07-07) — milestone 63 parser corpus sweep (Issues #8961/#9046)

Sweep on the same 673-file upstream corpus after the milestone 63 parser sweep:

| Metric | 2026-07-03 | 2026-07-07 | Delta |
|--------|------------|------------|-------|
| Files swept | 673 | 673 | — |
| Parsed cleanly | 639 (94.95%) | 654 (97.18%) | **+15** |
| Files with parse errors | 34 | 19 | **−15** |
| Parser panics | 0 | 0 | — |
| Divergence records | 235 | 57 | **−178** |

Issue #8961 is removed from `PARSER_CORPUS_ALLOWLIST.toml`: the parser now
accepts the surfaced base/stdlib/test gaps for relative macro imports, macro
generator bodies, qualified non-standard string literals, interpolated macro
names, and octal/quote char literals. Issue #9046 is also removed: current
`julia/test/syntax.jl` parses cleanly, covering empty ncat literals, string/cmd
literal suffixes, qualified macro `do` calls, macro braces with optional space,
dotted unary operators, exotic operator suffixes, and slurp-argument defaults.

The same sweep also ratcheted four older #8753 allowlist entries
(`InteractiveUtils/test/runtests.jl`, `REPL/src/docview.jl`,
`test/keywordargs.jl`, `test/precompile.jl`) because they now parse cleanly.

## Update (2026-07-14) — upstream-derived operator character set (Issue #11083)

Sweep on the same 673-file upstream corpus after the lexer's operator character
set was derived from upstream's precedence tables (`julia/src/julia-parser.scm`)
instead of an ad-hoc allowlist, and operator names gained upstream's operator
suffixes (`jl_op_suffix_char`):

| Metric | 2026-07-07 | 2026-07-14 | Delta |
|--------|------------|------------|-------|
| Files swept | 673 | 673 | — |
| Parsed cleanly | 658 (97.77%) | 659 (97.92%) | **+1** |
| Files with parse errors | 15 | 14 | **−1** |
| Parser panics | 0 | 0 | — |
| Divergence records | 55 | 54 | **−1** |

`julia/test/bitarray.jl` now parses cleanly (it uses the previously unlexable
operator glyphs) and its `#8753` entry is removed from
`PARSER_CORPUS_ALLOWLIST.toml`. No previously clean file regressed.
