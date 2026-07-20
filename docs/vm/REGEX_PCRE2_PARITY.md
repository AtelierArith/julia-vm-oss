# Regex PCRE2 Parity Checklist (fancy-regex engine)

**Issue #10080** — systematic inventory of upstream Julia (PCRE2) vs sjulia
(pure-Rust `fancy-regex`, since PR #10079 / Issue #8992) Regex semantics.

Upstream Julia compiles patterns with PCRE2 options
`UTF | MATCH_INVALID_UTF | ALT_BSUX | UCP` and matches with
`NO_UTF_CHECK`. sjulia compiles the same pattern text with `fancy-regex`
(Unicode-by-default). Every row below was probed on 2026-07-10 with
`julia 1.12.6` vs `target/release/sjulia` (main), one MWE per row; the MWEs
live in the linked fixtures and Issues.

Status legend:

- **OK** — identical behavior; pinned by a fixture in
  `subset_julia_vm/tests/fixtures/regex/`.
- **UNSUPPORTED** — runs in upstream julia, errors in sjulia (`unsupported-feature` Issue).
- **WRONG** — runs in both, sjulia output differs (`bug` Issue).
- **DIVERGENT** — sjulia succeeds where upstream julia errors (sjulia more
  permissive). **Decided as a permanent engine-boundary divergence** — see
  "Permissiveness divergences — policy decision" below (Issue #10183); these
  rows are not slated to move to OK.

## Pattern syntax & matching semantics

| Feature | julia (PCRE2) | sjulia (fancy-regex) | Status | Fixture / Issue |
|---------|---------------|----------------------|--------|-----------------|
| Backreferences `(a)\1` | match | match | OK | `regex_backref_lookaround_8992.jl` |
| Lookahead `(?=)` `(?!)` | match | match | OK | `regex_backref_lookaround_8992.jl` |
| Fixed-length lookbehind `(?<=)` `(?<!)` | match | match | OK | `regex_backref_lookaround_8992.jl` |
| Variable-length lookbehind `(?<=ab*)c` | compile error (`length of lookbehind assertion is not limited`) | matches | DIVERGENT | #10183 |
| Atomic groups `(?>a+)b` | match | match | OK | `regex_atomic_possessive_conditional_10080.jl` |
| Possessive quantifiers `a*+` `\d++` | match | match | OK | `regex_atomic_possessive_conditional_10080.jl` |
| `\K` keep-out | match resets start | same | OK | `regex_atomic_possessive_conditional_10080.jl` |
| Conditional groups `(?(1)b\|c)` | match | match | OK | `regex_atomic_possessive_conditional_10080.jl` |
| Recursion `(?R)` / `(?0)` | balanced match | compile error (`recursion is not supported`) | UNSUPPORTED | `regex_recursion_reject_10181.jl` / #10181 |
| Group recursion `(?1)` / `(?+1)` / `(?-1)` | match | compile error (`recursion is not supported`) | UNSUPPORTED | `regex_recursion_reject_10181.jl` / #10181 |
| Named subroutine `(?&name)` / `(?P>name)` | match | compile error (`recursion is not supported`) | UNSUPPORTED | `regex_recursion_reject_10181.jl` / #10181 |
| Anchors `\A` `\z` `\Z` `\G` | match | match | OK | `regex_anchors_inline_flags_10080.jl` |
| Inline flags `(?i)` `(?i:...)` `(?s)` `(?m)`, mid-pattern `a(?i)b` | match | match | OK | `regex_anchors_inline_flags_10080.jl` |
| Literal flags `r"..."i/m/s` | match | match | OK | `regex_flag_literals_5709.jl` |
| Extended flag `r"..."x` (whitespace + `#` comments) | match | match | OK | `regex_anchors_inline_flags_10080.jl` |
| Comment groups `(?#...)` | ignored | ignored | OK | `regex_anchors_inline_flags_10080.jl` |
| Lazy / counted quantifiers `.+?` `{2,3}` `{2,}?` | match | match | OK | `regex_eachmatch_classes_10080.jl` |
| Named-capture *matching* `(?<name>...)` | match | match | OK | probed via #10173 MWE (access surface is the gap) |

## Character classes & escapes

| Feature | julia (PCRE2) | sjulia (fancy-regex) | Status | Fixture / Issue |
|---------|---------------|----------------------|--------|-----------------|
| `\p{L}` `\p{Lu}` `\p{Greek}` `\P{L}` | Unicode property match | same | OK | `regex_unicode_fold_props_10080.jl` |
| Case-insensitive Unicode folding (`é`, `Ω`, `σ`, Kelvin `K`) | simple fold | same (incl. no full `ß`→`ss` fold, no `İ`→`i`) | OK | `regex_unicode_fold_props_10080.jl` |
| Unicode `\w` / `\b` (UCP) | Unicode-aware | same | OK | `regex_unicode_fold_props_10080.jl` |
| `\R` (any newline) | class match | same | OK | `regex_eachmatch_classes_10080.jl` |
| `\N` (not newline) | class match | same | OK | `regex_eachmatch_classes_10080.jl` |
| `\h` / `\H` (horizontal whitespace) | class match | class match (rewritten at construction) | OK | `regex_escape_classes_10179.jl` |
| `\v` / `\V` (vertical whitespace class) | `\n` etc. match | class match (rewritten at construction) | OK | `regex_escape_classes_10179.jl` |
| `\x41` hex escape | match | match | OK | `regex_eachmatch_classes_10080.jl` |
| `\x{3042}` braced hex | **no match** (ALT_BSUX: ECMAScript `\x`) | matches U+3042 | DIVERGENT | #10183 |
| Octal `\101` | matches `A` | matches `A` (rewritten to `\x{..}`) | OK | `regex_escape_classes_10179.jl` |
| `\o{101}` | matches `A` | matches `A` (rewritten to `\x{..}`) | OK | `regex_escape_classes_10179.jl` |
| Control escape `\cA` | matches U+0001 | matches U+0001 (rewritten to `\x{..}`) | OK | `regex_escape_classes_10179.jl` |

## API surface (Base functions taking Regex)

| Feature | julia (PCRE2) | sjulia | Status | Fixture / Issue |
|---------|---------------|--------|--------|-----------------|
| `match` / `eachmatch` / `occursin` 2-arg | works | works | OK | `match.jl`, `eachmatch.jl`, `occursin.jl` |
| `match(re, s, start)` 3-arg | works | works | OK | `regex_constructor_3arg_match_10178.jl` (#10178) |
| `Regex(pattern)` / `Regex(pattern, flags)` constructor | works | works | OK | `regex_constructor_3arg_match_10178.jl` (#10178) |
| `m[i]` integer capture indexing | works | works | OK | `regex_match_surface_10173_10182.jl` (#10173) |
| `m[:name]` / `m["name"]` / `keys(m)` / `haskey(m, k)` | works | works | OK | `regex_match_surface_10173_10182.jl` (#10173) |
| `r.pattern` field access | works | works | OK | `regex_match_surface_10173_10182.jl` (#10173) |
| `m.match` / `m.offset` / `m.captures` element access | works | works | OK | `match_field_access.jl` |
| `m.captures` / `m.offsets` container type | `Vector` | `Vector` | OK | `regex_match_surface_10173_10182.jl` (#10182) |
| `show(m)` format | `RegexMatch("a", 1="a")` | same | OK | `regex_match_surface_10173_10182.jl` (#10182) |
| `count(re, s)` / `findall(re, s)` | works | works | OK | `regex_count_findall_6749.jl` |
| `findfirst(re, s)` / `findnext(re, s, i)` | byte range | works | OK | `regex_findfirst_findnext_10177.jl` |
| `findlast(re, s)` / `rsplit(s, re)` | MethodError upstream too | error | OK (both error) | — |
| `split(s, re)` (+ `limit` / `keepempty`) | works | works | OK | `regex_split_10176.jl` |
| `startswith` / `endswith` with Regex | works | works | OK | `regex_endswith_5676.jl` |
| `replace(s, re => "str")` (+ positive `count`) | works | works | OK | `regex_replace_empty_advance_10080.jl` |
| `replace` zero-width-match advancement | steps 1 char | same | OK | `regex_replace_empty_advance_10080.jl` |
| `replace(...; count=0)` | replaces none | replaces none | OK | `regex_replace_surface_10174.jl` (#10197) |
| `replace(s, re => s"\1\g<n>\0")` capture refs | substituted | substituted | OK | `regex_replace_surface_10174.jl` (#10174) |
| `replace(s, re => f::Function)` | works | works | OK | `regex_replace_surface_10174.jl` (#10175) |
| `replace(s, p1 => r1, p2 => r2)` multi-pair | works | works | OK | `regex_replace_surface_10174.jl` (#10175) |
| `eachmatch(...; overlap=true)` | overlapping matches | overlapping matches | OK | `regex_eachmatch_overlap_10199.jl` (#10199) |
| `eachmatch` zero-width iteration | match per boundary | same | OK | `regex_eachmatch_classes_10080.jl` |
| `collect(eachmatch(...))` | `Vector{RegexMatch}` | matches materialized (container eltype `Vector{Any}`, values match) | OK | `regex_collect_eachmatch_10198.jl` (#10198) |
| Invalid-pattern error class | `ErrorException` at construction | `ErrorException`-class error | OK (coarse) | — |

## Runtime limits / error behavior

| Feature | julia (PCRE2) | sjulia (fancy-regex) | Status | Fixture / Issue |
|---------|---------------|----------------------|--------|-----------------|
| Catastrophic backtracking `(a\|a?)+$` on `"a"^28*"b"` | `PCRE.exec error: match limit exceeded` | completes, returns the (valid) empty match | DIVERGENT | #10183 |
| fancy-regex backtrack limit | n/a | own limit, different threshold and error text | DIVERGENT | #10183 |

## Permissiveness divergences — policy decision (Issue #10183)

Three probed behaviors are cases where **sjulia (fancy-regex) accepts or
completes what upstream Julia (PCRE2) rejects with an error** — sjulia is
strictly *more* permissive:

1. **Variable-length lookbehind** `(?<=ab*)c` — PCRE2 rejects unbounded
   lookbehind at compile time (`length of lookbehind assertion is not limited`);
   fancy-regex supports it and matches.
2. **`\x{...}` under `PCRE2_ALT_BSUX`** — upstream compiles with `ALT_BSUX`, so
   `\x{3042}` is read with ECMAScript `\x` semantics and does **not** match
   U+3042; fancy-regex reads `\x{HHHH}` as a Unicode code-point escape and
   matches. (The two-digit `\xHH` form matches in both engines.)
3. **Match-limit / catastrophic backtracking** `(a|a?)+$` on `"a"^28*"b"` —
   PCRE2 raises `match limit exceeded`; fancy-regex has its own backtrack budget
   with a different threshold, so on this input it completes and returns the
   valid empty match (and on other inputs it can error with different text at a
   different size).

**Decision (accepted 2026-07-12): declare all three DIVERGENT — permanent
engine-boundary differences; do NOT enforce error parity.** Rationale:

- The regex surface is **Native Boundary Policy A** (pure-Rust `fancy-regex`,
  *not* PCRE2 as a native dependency; Issue #8992, `NATIVE_BOUNDARY.md`). These
  are the chosen engine's accept/limit envelopes, not sjulia lowering bugs.
- Cases 2 (`ALT_BSUX \x`) and 3 (backtrack-limit threshold and error text) are
  **intrinsic to the engine**: matching them exactly would require
  reimplementing PCRE2's `ALT_BSUX` pattern rewriting and its match-limit
  accounting inside a *different* engine — which the ADR explicitly excludes
  ("Do not attempt to reimplement PCRE2 semantics"). They are effectively
  permanent.
- Case 1 (variable-length lookbehind) *could* be rejected at construction, but
  only by re-parsing/analyzing the pattern to decide whether each lookbehind is
  length-bounded — an ad-hoc, false-positive-prone heuristic (character classes,
  escapes, nested groups) that contradicts the "General Over Ad-hoc" principle.
  fancy-regex's broader lookbehind support is a feature superset, not a
  correctness bug, so no rejection is added.

Consequence: programs that upstream Julia rejects with a PCRE compile/exec error
may run under sjulia. This is the accepted behavior; the DIVERGENT rows above are
**not** slated to move to OK. The current sjulia behavior for all three is pinned
by the characterization test `test_pcre2_permissiveness_divergences_10183` in
`subset_julia_vm_bytecode/src/value/regex.rs`; if a future `fancy-regex` upgrade
changes any of them, update that test and this section together.

## Summary (2026-07-10 audit)

- ~40 probed behaviors, essentially all now **OK**. The 2026-07-10 audit
  filed 15 gap Issues; milestone 71 ("Regex & String Surface Compatibility")
  resolved them (2026-07-12):
  - Escape-class cluster #10179 `\ddd`/`\o{}`/`\cX`, #10180 `\v`, #10203
    `\h`/`\H` — translated at `Regex` construction (`regex_escape_classes_10179.jl`).
  - Base API surface — #10176 `split(::Regex)`, #10177 `findfirst`/`findnext`,
    #10178 `Regex()` ctor + 3-arg `match`, #10173/#10182 `RegexMatch`
    indexing / `keys` / `haskey` / `r.pattern` / Vector captures / `show`,
    #10174/#10175/#10197 substitution refs / Function & multi-pair `replace` /
    `count=0`, #10198 `collect(eachmatch)`, #10199 `overlap=true`.
  - #10181 recursion `(?R)`/`(?n)`/`(?&name)` — **UNSUPPORTED by decision**:
    rejected at construction with a clear error (fancy-regex cannot do
    recursion), pinned by `regex_recursion_reject_10181.jl`.
  - #10183 — **3 DIVERGENT** cases where sjulia is more permissive than PCRE2,
    decided as a permanent engine-boundary divergence (see the "Permissiveness
    divergences — policy decision" section above).
- Engine-level constructs (lookaround, backrefs, atomic/possessive,
  conditionals, `\K`, anchors, flags, Unicode classes/folding, and the
  `\h` `\v` octal `\o{}` `\cX` escape-class translations) plus the full Base
  API surface (`Regex` constructor, `split`, `findfirst`/`findnext`,
  `RegexMatch` indexing, substitution references) are all in parity under
  fancy-regex. The only non-OK rows are the deliberate #10181 recursion
  rejection and the #10183 permissiveness divergences.

When a linked Issue is fixed, move its row to OK and pin the behavior with a
fixture in `subset_julia_vm/tests/fixtures/regex/`.
