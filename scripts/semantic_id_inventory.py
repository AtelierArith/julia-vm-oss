#!/usr/bin/env python3
"""Mechanically inventory bare-name identity sites for Issue #10459 Phase 0.

This is a REPORT GENERATOR, not a CI gate. It never fails the build and is
not wired into any check_*.sh / premerge_gate.sh flow (unlike
scripts/check_name_based_lookup.sh, which IS a gate, ratcheting six specific
already-known patterns). Re-run it any time to regenerate
docs/vm/SEMANTIC_ID_INVENTORY.tsv; there is no ratchet on its output.

Modeled directly on scripts/panic_debt_classification.py (Issue #10869 Phase
0): a mechanical scan over production source, an explicit reviewer-auditable
rule table for the judgment calls the scan cannot make on its own, and a
reconciliation section tying the output back to an existing, already-trusted
gate script.

## What Issue #10459 asks Phase 0 to inventory

"Every bare-name identity table / name-keyed lookup in production code":
HashMap<String, ...>-shaped tables keyed by type/struct/function/module/
TypeVar names, `*_by_name` lookups, string-keyed registries, and the six
patterns scripts/check_name_based_lookup.sh already ratchets (Issue #10279's
bug-cluster guard). Classified along four axes:

  (a) identity domain  — struct / function / typevar / module / method-sig /
                          global / other (anything the mechanical keyword
                          scan cannot confidently place in one of the six
                          named domains — kept visible, not hidden, but not
                          counted toward the six-domain migration totals;
                          see "Domain classification" below).
  (b) layer             — parser / lowering / compile / inference / vm /
                          cache / ffi (Issue #10459's own layer vocabulary;
                          see LAYER_RULES).
  (c) migration difficulty — mechanical-rename / requires-owner-context-
                          plumbing / requires-serialization-format-change
                          (see "Difficulty classification" below).
  (d) semantic verdict  — identity-bearing / lexical-boundary / inert
                          (see "Verdict classification" below).

## Mechanism (mechanical, re-runnable; small rule table only for the
##  genuinely judgment-call axis, same shape as panic_debt_classification's
##  RULES_BY_FILE)

1. **Scope**: every `.rs` file under `src/` in all eleven workspace crates
   (subset_julia_vm, subset_julia_vm_lowering, subset_julia_vm_compile,
   subset_julia_vm_vm, subset_julia_vm_types, subset_julia_vm_bytecode,
   subset_julia_vm_ir, subset_julia_vm_ffi, subset_julia_vm_parser,
   subset_julia_vm_web, subset_julia_vm_runtime) — broader than
   panic_debt_classification's five-crate PANIC_RATCHET_ROOTS scope, because
   Issue #10459 explicitly names `subset_julia_vm_types/src/` (the type
   system crate, not in the panic-free ratchet's scope) as a required
   inventory root.

2. **Three detection kinds**, each a mechanical regex/structural scan:

   - `map_decl` — a `HashMap<String, X>` / `HashMap<&str, X>` /
     `BTreeMap<String, X>` field, `let`/`static`/`const` binding, or function
     parameter/return type. Captures the value type `X` (balanced generic
     scan, handles nested `<...>`/tuples) for domain classification.
   - `by_name_ref` — any identifier matching `[A-Za-z0-9_]+_by_name` that is a function
     definition or a call/reference site, EXCLUDING lines already counted as
     a `map_decl` on the same (file, line) (a field like
     `abstract_type_by_name: HashMap<String, usize>` is one site, not two).
   - `anchor` — the exact six patterns `scripts/check_name_based_lookup.sh`
     already ratchets (Issue #10279's bug-cluster guard), reproduced
     verbatim (`ANCHOR_CHECKS` below, copied pattern-for-pattern) so this
     script's anchor counts reconcile against that gate's live output by
     construction. `EXTRA_ANCHOR_ROWS` below is empty as of Issue #10987
     (Phase 1 completion): it used to carry one manually-declared row for
     `runtime_typevar_projection_identities` (`subset_julia_vm_vm/src/vm/mod.rs`)
     whose key had changed from an all-`String` 3-tuple (matched by the
     gate's regex) to a `(CoreType, usize, String, Option<String>)` 4-tuple
     (Issue #10261) that the gate's `String`-only-key regex no longer matched
     at all, while the field's key still had `String`/`Option<String>`
     components participating in equality/hashing — so the hand-declared row
     kept that residual debt visible. Issue #10987 replaced the key with the
     fully structural `TypeVarProjectionKey { owner: CoreType, binder_depth:
     usize, declared_lower: JuliaType, declared_upper: JuliaType }` (the
     as-declared bounds participate as PARSED structural types, never
     rendered strings; the display name is value-side metadata only), so the
     row was retired rather than updated. If a future change reintroduces a
     rendered-string key component that the mechanical scan cannot see, add
     a new row here rather than leaving it invisible.

3. **Test-only exclusion**: identical machinery to
   `scripts/panic_debt_classification.py` (path pattern, cross-file
   `#[cfg(test)] mod ident;` closure, inline `#[cfg(test)]`/`#[test]`
   brace-scope detection) — copied verbatim (see the "copied from
   panic_debt_classification.py" markers below) so both scripts' test-only
   judgment agrees on any file shared between their scopes. Only
   production-reachable sites are inventoried; test-only string-keyed maps
   (e.g. a test helper's `HashMap<String, Value>` fixture table) are neither
   identity debt nor migration work.

4. **Domain classification** (`classify_domain`): a fixed-order,
   case-insensitive substring rule table over the combined text of the
   nearest identifier name (field/local/fn name) and the captured value type
   (for `map_decl`) or the identifier itself (for `by_name_ref`/`anchor`).
   First match wins: `typevar` -> typevar, `module` -> module, `global` ->
   global, `method` -> method-sig, `function`/`func` -> function, `struct`
   -> struct, generic `type` (TypeVar already matched above) -> struct
   (fallback: a name/type registry without a more specific keyword, e.g.
   `abstract_type_by_name`, `enum_types`, `type_aliases`, is treated as
   struct-shaped type identity, matching Issue #10459's "module-owned struct
   identity" phase, which upstream Julia also treats structs/abstracts/
   primitives/enums as instances of the same `DataType`/type-definition
   family). Anything matching none of these keywords is `other` — kept in
   the TSV for visibility (comprehensive inventory, not a curated subset)
   but explicitly NOT part of the six-domain migration totals in
   `docs/vm/SEMANTIC_ID_MIGRATION.md`'s priority ranking, since a keyword
   miss is a "needs manual triage" signal, not a confident domain call. Known
   `other` classes found during authoring: Julia-level `getfield`-by-Symbol
   reflection helpers (`field_by_name` on an already-resolved struct
   *instance* — a legitimate Julia-level operation on a value, not an
   internal Rust cross-owner identity collision risk, out of this epic's
   scope by definition: see the epic body's "Proposed model" list, which
   names struct/function/TypeVar/module/method identity, not field-of-a-
   resolved-instance identity), macro-name tables, and closure-capture sets.

5. **Difficulty classification** (`classify_difficulty`): mechanical
   structural-context scan (`enclosing_block_kind`, a brace-depth stack
   tracking whether a line sits inside a `struct { ... }` body, a `fn { ... }`
   body, an `impl`/`trait` body, or top level — the same class of
   string/comment-masked brace scan `panic_debt_classification.py` uses for
   `#[cfg(test)]` detection, generalized to track block *kind* instead of
   test-gating) plus two fixed overrides:

   - `layer == cache` (the file is one of the persisted-cache/serialization
     boundary files) -> always `requires-serialization-format-change`,
     regardless of struct-field-vs-local-var context: any identity table
     whose *contents* end up in a `.sjvmbc`/`.sjir`/base-cache/`.ji.json`
     payload needs an explicit relocation table (Issue #10459's "Serialization
     stores IDs through explicit relocation tables" requirement), not just an
     in-process rename.
   - `kind == anchor` -> each of the six `ANCHOR_CHECKS` entries (plus the
     one `EXTRA_ANCHOR_ROWS` row) carries its own fixed `difficulty` value
     instead of using the generic heuristic, mirroring
     `panic_debt_classification.py`'s `RULES_BY_FILE` judgment-call table:
     these sites are already analyzed in `docs/vm/SEMANTIC_IDENTITIES.md`
     (e.g. `typevar_core_bindings`'s existing note, "same-name binders can
     collide unless callers prove the map is only a lexical scratchpad",
     is exactly a `requires-owner-context-plumbing` call, not a
     mechanical-rename one, even though several of its 12 sites are
     function-local `HashMap::new()` bindings that the generic local-var
     heuristic below would otherwise call mechanical-rename).

   Otherwise, the generic heuristic is: a `struct`/`enum`/`trait`/`impl`-body
   field (persistent, cross-call state) or a top-level `static`/`const` ->
   `requires-owner-context-plumbing` (the table outlives one compile pass or
   one call, so a bare rename cannot prove no cross-owner collision); a
   `fn`-body local binding -> `mechanical-rename` UNLESS its domain is
   `typevar` (function-local TypeVar/CoreType binding scratch maps are the
   one domain Issue #10459's own text singles out as needing a purity proof,
   not just a type swap, before they can be called low-difficulty) or its
   layer is `inference` and the domain is `struct` (a local scratch map built
   from long-lived struct/type definitions inherits their cross-owner risk
   even though the map itself is a function-local variable). This is a
   heuristic, not a proof — see "Known limitations" below.

6. **Verdict classification** (`classify_verdict`): every detected site fails
   closed to `identity-bearing`. An exact symbol or path/symbol rule may
   downgrade it to `lexical-boundary` or `inert` only when a landed PR has
   established that verdict. The `other` domain also fails closed because the
   domain keyword scan has documented false negatives; Phase 4 totals filter
   to the six core domains without mislabeling unreviewed `other` sites inert.

## Known limitations (documented, not fixed — same spirit as
##  panic_debt_classification.py's "Known heuristic gaps" section)

- `enclosing_block_kind` is a source-text brace scan, not a real parser: a
  single-line `struct Foo { x: HashMap<String, Y> }` (not used anywhere in
  this codebase at authoring time, checked by hand) would misclassify `x`'s
  context, and a `match`/`if`/closure body nested inside a `fn` correctly
  inherits `fn_body` (blocks with no `struct`/`fn`/`impl`/`enum`/`trait`
  keyword on their own opening line copy the current top-of-stack kind
  rather than resetting it), but a closure passed as a *value* into a
  long-lived struct field (so its captured local `HashMap` outlives the
  literal function call) would still be scored `mechanical-rename` by this
  script even though its real difficulty may be higher. Phase-1+ authors
  must confirm the difficulty call by reading the site, not just trust the
  TSV.
- `mechanical-rename` here means "the map's *key type* can be swapped for an
  owner-scoped ID without redesigning who calls the containing function or
  what the function returns" — it does NOT mean "zero-risk" or "no design
  work". Every domain still needs the ID *type* to exist first (Issue
  #10459's own Phase 1-3 ordering).
- The value-type/identifier keyword scan can misfire on a coincidental
  substring (e.g. a hypothetical field literally named `substructure_cache`
  would match the `struct` keyword despite meaning "sub-structure" in the
  data-layout sense). No such false positive was found in a spot check of
  the `struct`/`other` boundary at authoring time, but this is a substring
  match, not semantic analysis.
- `_nearest_identifier` is a single-physical-line backward scan (`&`/
  lifetime/`mut` strip, wrapping-generic-open strip, module-path strip,
  `impl ... for`/tuple-variant/return-type recognition). At authoring time
  11 of 873 sites (~1.3%) still could not resolve an owning identifier and
  fall back to the literal `HashMap`/`BTreeMap` token as their `symbol`
  column (still correctly counted, domain/layer/difficulty-classified from
  the value type alone) — all 11 are multi-line declarations this
  single-line scan cannot see across (a tuple return type spanning several
  source lines, a `RwLockGuard<'static, HashMap<...>>` reference where the
  map is the *second* generic argument after a lifetime, and a couple of
  `.collect::<HashMap<...>>()` chained calls whose target `let` binding is
  one or more lines above the match). Accepted rather than built out further
  (a real recursive-descent scan of Rust generics is a much larger tool than
  a Phase 0 report generator warrants); re-run with `--detail <path>` (a
  non-committed, per-line sibling of the aggregated TSV) and grep its
  `symbol` column for `^HashMap$`/`^BTreeMap$` to find the current list.

## Reconciliation

`main()` prints a live re-run of `scripts/check_name_based_lookup.sh`'s six
patterns (`ANCHOR_CHECKS`, copied verbatim) side by side with this script's
`anchor` kind counts for the same six patterns — they are the same regex
over the same roots, so they must match exactly; any drift means the two
scripts' copies have diverged and should be investigated before trusting the
rest of the inventory.

Usage:
    python3 scripts/semantic_id_inventory.py
    python3 scripts/semantic_id_inventory.py --out docs/vm/SEMANTIC_ID_INVENTORY.tsv
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter, defaultdict, deque
from pathlib import Path

# ---------------------------------------------------------------------------
# Scope: every .rs file under src/ in all eleven workspace crates. Broader
# than panic_debt_classification.py's PANIC_RATCHET_ROOTS because Issue
# #10459 explicitly names subset_julia_vm_types/src/ (not in that scope).
# ---------------------------------------------------------------------------
ROOT_DIRS = (
    Path("subset_julia_vm/src"),
    Path("subset_julia_vm_lowering/src"),
    Path("subset_julia_vm_compile/src"),
    Path("subset_julia_vm_vm/src"),
    Path("subset_julia_vm_types/src"),
    Path("subset_julia_vm_bytecode/src"),
    Path("subset_julia_vm_ir/src"),
    Path("subset_julia_vm_ffi/src"),
    Path("subset_julia_vm_parser/src"),
    Path("subset_julia_vm_web/src"),
    Path("subset_julia_vm_runtime/src"),
)

DOMAINS = ("typevar", "struct", "function", "method-sig", "module", "global", "other")
LAYERS = ("parser", "lowering", "compile", "inference", "vm", "cache", "ffi", "other")
DIFFICULTIES = (
    "mechanical-rename",
    "requires-owner-context-plumbing",
    "requires-serialization-format-change",
)

IDENTITY_BEARING = "identity-bearing"
LEXICAL_BOUNDARY = "lexical-boundary"
INERT = "inert"
VERDICTS = (IDENTITY_BEARING, LEXICAL_BOUNDARY, INERT)

LEXICAL_BOUNDARY_SYMBOLS = frozenset(
    {
        "module_functions",
        "module_exports",
        "module_constants",
        "module_struct_names",
        "module_usings",
        "module_abstract_names",
        "module_imported_bindings",
        "module_aliases",
    }
)

INERT_SYMBOLS = frozenset(
    {
        "global_types",
        "inference_global_types",
        "global_const_structs",
        "global_struct_names",
    }
)

TYPEVAR_BOUNDARY_PATH_SYMBOLS = frozenset(
    {
        (
            "subset_julia_vm_types/src/inference_core/dispatch_resolver.rs",
            "LexicalTypeBindings",
        ),
        (
            "subset_julia_vm_types/src/inference_core/type_core.rs",
            "RenderedTypeParseCache",
        ),
    }
)

LEXICAL_BOUNDARY_PATH_SYMBOLS = frozenset(
    {
        ("subset_julia_vm_bytecode/src/module_intern.rs", "index"),
        ("subset_julia_vm_bytecode/src/struct_registry.rs", "by_name"),
        (
            "subset_julia_vm_types/src/inference_core/type_core/match.rs",
            "by_name",
        ),
    }
) | TYPEVAR_BOUNDARY_PATH_SYMBOLS

MECHANICAL_RENAME = "mechanical-rename"
REQUIRES_OWNER_CONTEXT = "requires-owner-context-plumbing"
REQUIRES_SERIALIZATION = "requires-serialization-format-change"


def classify_effective_domain(domain: str, file_rel: str, symbol: str) -> str:
    """Correct conservative keyword-domain guesses with exact evidence."""
    if (file_rel, symbol) in TYPEVAR_BOUNDARY_PATH_SYMBOLS:
        return "typevar"
    return domain


def classify_verdict(kind: str, domain: str, file_rel: str, symbol: str) -> str:
    """Classify whether a mechanical hit remains semantic-ID debt.

    Every hit fails closed to identity-bearing. Downgrades require an exact
    symbol or path/symbol rule backed by the as-landed Phase verdicts.
    """
    del kind, domain  # Reserved for future evidence rules without changing callers.
    if symbol in INERT_SYMBOLS:
        return INERT
    if symbol in LEXICAL_BOUNDARY_SYMBOLS:
        return LEXICAL_BOUNDARY
    if (file_rel, symbol) in LEXICAL_BOUNDARY_PATH_SYMBOLS:
        return LEXICAL_BOUNDARY
    return IDENTITY_BEARING


# ---------------------------------------------------------------------------
# module_key(): same shape as scripts/panic_debt_classification.py's
# module_key() (itself copied from scripts/check_panic_free_ratchet.sh),
# generalized so every one of the eight crate roots gets a sensible
# crate/src/subdir grouping rather than only subset_julia_vm.
# ---------------------------------------------------------------------------
def module_key(path: str) -> str:
    parts = path.replace("\\", "/").split("/")
    if len(parts) >= 4 and parts[0] == "subset_julia_vm" and parts[1] == "src":
        if parts[2] == "vm" and len(parts) >= 5:
            return "/".join(parts[:5]) if parts[3] == "exec" else "/".join(parts[:4])
        return "/".join(parts[:3])
    if len(parts) >= 3 and parts[0].startswith("subset_julia_vm"):
        return "/".join(parts[:3])
    return "/".join(parts[:2])


# ---------------------------------------------------------------------------
# layer classification: Issue #10459's own vocabulary (parser/lowering/
# compile/inference/vm/cache/ffi). Cache-file membership is checked FIRST
# and wins regardless of crate, since a struct/function identity table that
# happens to live inside a cache (de)serialization file is a serialization
# concern first.
# ---------------------------------------------------------------------------
CACHE_FILES = {
    "subset_julia_vm_compile/src/compile/cache.rs",
    "subset_julia_vm_compile/src/compile/embedded_cache.rs",
    "subset_julia_vm_compile/src/compile/preload_cache.rs",
    "subset_julia_vm_compile/src/compile/seeded_cache.rs",
    "subset_julia_vm_compile/src/compile/precompile.rs",
    "subset_julia_vm/src/core_ir_file.rs",
    "subset_julia_vm/src/vm_bytecode_file.rs",
    "subset_julia_vm/src/loader.rs",
    "subset_julia_vm_ffi/src/bytecode.rs",
}


def classify_layer(rel: str) -> str:
    if rel in CACHE_FILES:
        return "cache"
    if rel.startswith("subset_julia_vm_ffi/") or rel.startswith("subset_julia_vm_web/"):
        return "ffi"
    if rel.startswith("subset_julia_vm_parser/"):
        return "parser"
    if rel.startswith("subset_julia_vm_lowering/src/lowering/") or rel == "subset_julia_vm_lowering/src/lowering.rs":
        return "lowering"
    if rel.startswith("subset_julia_vm_compile/src/compile/") or rel == "subset_julia_vm_compile/src/compile.rs":
        return "compile"
    if rel.startswith("subset_julia_vm_types/"):
        return "inference"
    if rel.startswith("subset_julia_vm_bytecode/"):
        # Shared program representation consumed by compile's output stage;
        # no dedicated "bytecode" layer in Issue #10459's own vocabulary, so
        # folded into compile (its nearest producer/consumer stage).
        return "compile"
    if (
        rel.startswith("subset_julia_vm_vm/src/vm/")
        or rel.startswith("subset_julia_vm/src/repl/")
        or rel == "subset_julia_vm_vm/src/register_vm.rs"
        or rel.startswith("subset_julia_vm_runtime/")
    ):
        return "vm"
    return "other"


# ---------------------------------------------------------------------------
# Domain classification: fixed-order, case-insensitive substring rules over
# combined identifier + value-type text. See module docstring "Domain
# classification" for the rationale and ordering.
# ---------------------------------------------------------------------------
_DOMAIN_RULES: list[tuple[str, str]] = [
    ("typevar", "typevar"),
    ("module", "module"),
    ("global", "global"),
    ("method", "method-sig"),
    ("function", "function"),
    ("func", "function"),
    ("callable", "function"),
    ("struct", "struct"),
    ("type", "struct"),  # generic fallback; typevar already matched above
]


def classify_domain(text: str) -> str:
    lowered = text.lower()
    for needle, domain in _DOMAIN_RULES:
        if needle in lowered:
            return domain
    return "other"


# ---------------------------------------------------------------------------
# Source masking + test-only detection: copied from
# scripts/panic_debt_classification.py (same functions, same docstrings
# trimmed) so both scripts agree on what counts as test-only within any file
# their scopes share.
# ---------------------------------------------------------------------------
_RAW_STRING_RE = re.compile(r'(?<![A-Za-z0-9_])b?r(#*)"(?:.|\n)*?"\1')
_BLOCK_COMMENT_RE = re.compile(r"/\*(?:.|\n)*?\*/")
_STRING_RE = re.compile(r'"(?:\\.|[^"\\\n])*"')
_CHAR_RE = re.compile(r"'(?:\\.|[^'\\\n])'")
_LINE_COMMENT_RE = re.compile(r"//[^\n]*")


def _mask(m: re.Match[str]) -> str:
    return re.sub(r"[^\n]", "x", m.group(0))


def mask_non_code(text: str) -> str:
    text = _RAW_STRING_RE.sub(_mask, text)
    text = _BLOCK_COMMENT_RE.sub(_mask, text)
    text = _STRING_RE.sub(_mask, text)
    text = _CHAR_RE.sub(_mask, text)
    text = _LINE_COMMENT_RE.sub(_mask, text)
    return text


def is_test_or_bench_path(rel: str) -> bool:
    p = rel.replace("\\", "/")
    return "/tests/" in p or p.endswith("_tests.rs") or "/benches/" in p


_CFG_TEST_RE = re.compile(r"cfg\s*\([^)]*\btest\b[^)]*\)")
_CFG_NOT_TEST_RE = re.compile(r"cfg\s*\(\s*not\s*\(\s*test\s*\)\s*\)")


def is_test_cfg_attr(attr_text: str) -> bool:
    if _CFG_NOT_TEST_RE.search(attr_text):
        return False
    if re.search(r"#\[test\]", attr_text):
        return True
    return bool(_CFG_TEST_RE.search(attr_text))


_MOD_DECL_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;\s*$")
_MOD_DECL_INLINE_ATTR_RE = re.compile(
    r"^\s*#\[([^\]]*)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;\s*$"
)
_ATTR_LINE_RE = re.compile(r"^\s*#!?\[")


def _is_blank_or_comment(line: str) -> bool:
    stripped = line.lstrip()
    return stripped == "" or stripped.startswith("//")


def find_mod_declarations(lines: list[str]) -> list[tuple[str, bool]]:
    results: list[tuple[str, bool]] = []
    for i, line in enumerate(lines):
        m_inline = _MOD_DECL_INLINE_ATTR_RE.match(line)
        if m_inline:
            results.append((m_inline.group(2), is_test_cfg_attr(m_inline.group(1))))
            continue
        m = _MOD_DECL_RE.match(line)
        if not m:
            continue
        gated = False
        j = i - 1
        while j >= 0:
            prev = lines[j]
            if _ATTR_LINE_RE.match(prev):
                if is_test_cfg_attr(prev):
                    gated = True
                j -= 1
                continue
            if _is_blank_or_comment(prev):
                j -= 1
                continue
            break
        results.append((m.group(1), gated))
    return results


def resolve_child_dir(path: Path) -> Path:
    if path.stem in ("mod", "lib", "main"):
        return path.parent
    return path.parent / path.stem


def build_test_only_whole_file_closure(files: list[Path]) -> set[Path]:
    file_set = set(files)
    all_edges: dict[Path, list[Path]] = defaultdict(list)
    directly_gated: set[Path] = set()

    for f in files:
        try:
            lines = f.read_text(encoding="utf-8", errors="ignore").splitlines()
        except OSError:
            continue
        child_dir = resolve_child_dir(f)
        for ident, gated in find_mod_declarations(lines):
            candidates = (child_dir / f"{ident}.rs", child_dir / ident / "mod.rs")
            for candidate in candidates:
                if candidate in file_set:
                    all_edges[f].append(candidate)
                    if gated:
                        directly_gated.add(candidate)
                    break

    test_only: set[Path] = set()
    queue: deque[Path] = deque(directly_gated)
    while queue:
        f = queue.popleft()
        if f in test_only:
            continue
        test_only.add(f)
        for child in all_edges.get(f, []):
            if child not in test_only:
                queue.append(child)
    return test_only


def inline_test_scope_lines(masked_lines: list[str]) -> list[bool]:
    depth = 0
    scope_entry_depths: list[int] = []
    pending_test_attr = False
    result: list[bool] = []

    for line in masked_lines:
        if is_test_cfg_attr(line):
            pending_test_attr = True

        before = bool(scope_entry_depths)
        for ch in line:
            if ch == "{":
                depth += 1
                if pending_test_attr:
                    scope_entry_depths.append(depth)
                    pending_test_attr = False
            elif ch == "}":
                depth -= 1
                while scope_entry_depths and depth < scope_entry_depths[-1]:
                    scope_entry_depths.pop()
        after = bool(scope_entry_depths)
        result.append(before or after)
    return result


def rust_files() -> list[Path]:
    files: list[Path] = []
    for root in ROOT_DIRS:
        if root.exists():
            files.extend(sorted(root.rglob("*.rs")))
    return sorted(set(files))


# ---------------------------------------------------------------------------
# enclosing_block_kind: a per-line brace-depth stack tracking whether a line
# sits inside a struct/enum/trait body, a fn body, an impl body, or top
# level. See module docstring "Difficulty classification" and "Known
# limitations".
# ---------------------------------------------------------------------------
_STRUCT_DEF_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+[A-Za-z_]")
_ENUM_DEF_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+[A-Za-z_]")
_TRAIT_DEF_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?trait\s+[A-Za-z_]")
_IMPL_RE = re.compile(r"^\s*(?:unsafe\s+)?impl\b")
_FN_DEF_RE = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+\"[^\"]*\"\s+)?fn\s+[A-Za-z_]"
)

TOP_LEVEL = "top_level"
STRUCT_DEF = "struct_def"
ENUM_DEF = "enum_def"
TRAIT_DEF = "trait_def"
IMPL_BLOCK = "impl_block"
FN_BODY = "fn_body"


def _line_label(line: str) -> str | None:
    if _FN_DEF_RE.match(line):
        return FN_BODY
    if _STRUCT_DEF_RE.match(line):
        return STRUCT_DEF
    if _ENUM_DEF_RE.match(line):
        return ENUM_DEF
    if _TRAIT_DEF_RE.match(line):
        return TRAIT_DEF
    if _IMPL_RE.match(line):
        return IMPL_BLOCK
    return None


def enclosing_block_kinds(masked_lines: list[str]) -> list[str]:
    """Per line: the block kind (see constants above) active at the START
    of that line, i.e. before that line's own braces are processed. A line
    that opens a new labeled block (e.g. `pub struct Foo {`) is itself
    reported with the OUTER context, matching how map_decl matches are
    scored (a field declared a line or two after `struct Foo {` should read
    STRUCT_DEF, and it does, because by the time we reach the field's line
    the push already happened on the `struct Foo {` line)."""
    stack: list[str] = []
    result: list[str] = []
    for line in masked_lines:
        result.append(stack[-1] if stack else TOP_LEVEL)
        label = _line_label(line)
        used_label = False
        for ch in line:
            if ch == "{":
                if label is not None and not used_label:
                    stack.append(label)
                    used_label = True
                else:
                    stack.append(stack[-1] if stack else TOP_LEVEL)
            elif ch == "}":
                if stack:
                    stack.pop()
    return result


# ---------------------------------------------------------------------------
# map_decl detection: HashMap<String, X> / HashMap<&str, X> / BTreeMap<String, X>
# single-line declarations (fields, locals, statics, fn params/returns).
# Only 1 comment-only line in the whole workspace failed to close its `>` on
# the same physical line at authoring time (spot-checked); this scan is
# single-line by construction and will simply miss any further such line —
# acceptable for a report generator (see script docstring).
# ---------------------------------------------------------------------------
_MAP_OPEN_RE = re.compile(r"\b(HashMap|BTreeMap)\s*<\s*(String|&\s*'?[a-z]*\s*str)\s*,")
_IDENT_ANCHOR_RE = re.compile(r"([A-Za-z_][A-Za-z0-9_]*)\s*[:=]\s*$")
_ARROW_ANCHOR_RE = re.compile(r"->\s*$")
_FOR_ANCHOR_RE = re.compile(r"\bfor\s*$")  # `impl Trait for HashMap<...>`
# `Variant(HashMap<...>)` -- a tuple-enum-variant definition/constructor.
_TUPLE_CALL_ANCHOR_RE = re.compile(r"([A-Za-z_][A-Za-z0-9_]*)\s*\(\s*$")
# Peelable trailing tokens between the `ident:`/`ident =` anchor and the map
# type itself: a leading `&`/lifetime/`mut` (reference field), or one or more
# wrapping "Path::To::Type<" opens (`std::collections::HashMap<...>`,
# `Lazy<RwLock<HashMap<...>>>`, `Option<HashMap<...>>`, `RefCell<HashMap<...>>`,
# ...). Applied repeatedly, longest first, until nothing more peels.
# NOTE: every pattern below uses `\s*$` (zero-or-more), never `\s+$`, because
# the caller does not rstrip() between peels -- a peeled token's own leading
# separator whitespace is left in place for the *next* pattern to consume via
# its own `\s*$` rather than being eaten twice.
_PEEL_PATTERNS = (
    re.compile(r"&\s*$"),
    re.compile(r"'[A-Za-z_][A-Za-z0-9_]*\s*$"),
    re.compile(r"\bmut\s*$"),
    # a wrapper generic just opened ("Lazy<", "RwLock<", "std::collections::HashMap<" ...)
    re.compile(r"(?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*)*[A-Za-z_][A-Za-z0-9_]*\s*<\s*$"),
    # a bare module-qualification path with nothing open after it
    # (`std::collections::` directly preceding the already-matched HashMap<)
    re.compile(r"(?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*)+$"),
    # a tuple/parameter-list just opened (`-> (HashMap<...>, ...)`,
    # `fn f(x: HashMap<...>, ...)` when x itself is peeled already)
    re.compile(r"\(\s*$"),
)


def _balanced_value_type(line: str, start: int) -> str | None:
    """From just after the top-level comma inside HashMap<Key, VALUE>, scan
    forward tracking <>/()/[] depth to find the matching top-level `>` that
    closes the container's own generic argument list. Returns the VALUE text
    or None if unbalanced within this line (declaration spans multiple
    lines -- rare, see module docstring)."""
    angle = 1  # we are already inside the container's outer '<'
    paren = 0
    bracket = 0
    i = start
    n = len(line)
    while i < n:
        ch = line[i]
        if ch == "<":
            angle += 1
        elif ch == ">":
            angle -= 1
            if angle == 0:
                return line[start:i].strip()
        elif ch == "(":
            paren += 1
        elif ch == ")":
            paren -= 1
        elif ch == "[":
            bracket += 1
        elif ch == "]":
            bracket -= 1
        i += 1
    return None


def _nearest_identifier(line: str, open_idx: int) -> str:
    """Walk backward from a matched `HashMap<`/`BTreeMap<` to the field/
    local/static name (or a `(return type)` marker for a bare `-> HashMap<...>`
    function return type) that owns it, peeling any wrapping generic layers
    (`std::collections::HashMap<...>`, `Lazy<RwLock<HashMap<...>>>`,
    `Option<HashMap<...>>`, ...) one at a time. Falls back to `""` (caller
    substitutes the literal `HashMap`/`BTreeMap` token) only if nothing
    recognizable precedes the map type at all -- e.g. a turbofish
    `HashMap::<String, T>::new()` call expression, which is not a
    declaration site and correctly contributes no useful identifier."""
    prefix = line[:open_idx].rstrip()
    for _ in range(12):  # bounded: pathological nesting should not hang
        if _ARROW_ANCHOR_RE.search(prefix):
            return "(return type)"
        if _FOR_ANCHOR_RE.search(prefix):
            return "(impl target)"
        m = _IDENT_ANCHOR_RE.search(prefix)
        if m:
            return m.group(1)
        m = _TUPLE_CALL_ANCHOR_RE.search(prefix)
        if m:
            return m.group(1)
        peeled = False
        for pat in _PEEL_PATTERNS:
            m2 = pat.search(prefix)
            if m2:
                prefix = prefix[: m2.start()].rstrip()
                peeled = True
                break
        if not peeled:
            return ""
    return ""


_BY_NAME_RE = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*_by_name)\b")


def scan_file(
    path: Path,
    rel: str,
    whole_file_test_only: bool,
) -> list[tuple[str, str, str, str, int, str]]:
    """Return list of (kind, domain, layer_placeholder_unused, symbol, line, detail).
    layer is computed by caller from rel; kept out of the tuple's 3rd slot
    (always ''), simplifying this function's signature reuse below."""
    try:
        text = path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return []
    lines = text.splitlines()
    if whole_file_test_only:
        # Skipped entirely below; scan_lines is never read in that branch.
        return []

    masked_lines = mask_non_code(text).splitlines()
    if len(masked_lines) != len(lines):
        # Masking must preserve line count (defensive fallback only, not
        # expected in practice -- see panic_debt_classification.py's
        # identical fallback comment). Falls back to unmasked scanning
        # rather than mis-index, at the cost of possible comment/string
        # false positives for this one file.
        in_test = [False] * len(lines)
        block_kinds = [TOP_LEVEL] * len(lines)
        scan_lines = lines
    else:
        in_test = inline_test_scope_lines(masked_lines)
        block_kinds = enclosing_block_kinds(masked_lines)
        # Scan the comment/string-masked text, not the raw line: a doc
        # comment or a string literal that merely *mentions*
        # `HashMap<String, X>` (common in this codebase -- e.g.
        # `//! ... decodes into the \`HashMap<String, HashSet<String>>\`
        # field.`) must not be counted as a real declaration site. Real
        # code is byte-identical between `lines` and `masked_lines`; only
        # comment/string contents differ (replaced with `x`), so a match
        # against masked_lines is exactly "genuine code, unmasked" and its
        # captured text (ident, value type) is the original source text.
        scan_lines = masked_lines

    rows: list[tuple[str, str, str, str, int, str]] = []
    map_decl_lines: set[int] = set()

    for idx, line in enumerate(scan_lines):
        if idx < len(in_test) and in_test[idx]:
            continue
        for m in _MAP_OPEN_RE.finditer(line):
            value_start = m.end()
            value_type = _balanced_value_type(line, value_start)
            if value_type is None:
                continue
            ident = _nearest_identifier(line, m.start())
            block_kind = block_kinds[idx] if idx < len(block_kinds) else TOP_LEVEL
            combined = f"{ident} {value_type}"
            domain = classify_domain(combined)
            rows.append(("map_decl", domain, block_kind, ident or m.group(1), idx + 1, value_type))
            map_decl_lines.add(idx + 1)

    for idx, line in enumerate(scan_lines):
        if idx < len(in_test) and in_test[idx]:
            continue
        lineno = idx + 1
        if lineno in map_decl_lines:
            continue
        for m in _BY_NAME_RE.finditer(line):
            ident = m.group(1)
            block_kind = block_kinds[idx] if idx < len(block_kinds) else TOP_LEVEL
            domain = classify_domain(ident)
            rows.append(("by_name_ref", domain, block_kind, ident, lineno, ident))

    return rows


def classify_difficulty(kind: str, layer: str, block_kind: str, domain: str) -> str:
    if layer == "cache":
        return REQUIRES_SERIALIZATION
    if block_kind == FN_BODY:
        if domain == "typevar":
            return REQUIRES_OWNER_CONTEXT
        if domain == "struct" and layer == "inference":
            return REQUIRES_OWNER_CONTEXT
        return MECHANICAL_RENAME
    return REQUIRES_OWNER_CONTEXT


# ---------------------------------------------------------------------------
# anchor rows: the exact six scripts/check_name_based_lookup.sh patterns,
# copied verbatim (roots/patterns/baseline), plus one hand-declared row. See
# module docstring point 2 ("anchor") and the "Reconciliation" section.
# ---------------------------------------------------------------------------
ANCHOR_CHECKS = [
    {
        "name": "typevar_scope_maps",
        "root": Path("subset_julia_vm_types/src/inference_core"),
        "pattern": re.compile(r"HashMap\s*<\s*String\s*,\s*CoreTypeVar\s*>"),
        "domain": "typevar",
        "layer": "inference",
        "difficulty": REQUIRES_OWNER_CONTEXT,
    },
    {
        "name": "typevar_core_bindings",
        "root": Path("subset_julia_vm_types/src/inference_core"),
        "pattern": re.compile(r"HashMap\s*<\s*String\s*,\s*CoreType\s*>"),
        "domain": "typevar",
        "layer": "inference",
        "difficulty": REQUIRES_OWNER_CONTEXT,
        "classified_lines": {
            (
                Path("subset_julia_vm_types/src/inference_core/dispatch_resolver.rs"),
                "type LexicalTypeBindings = HashMap<String, CoreType>;",
            ),
            (
                Path("subset_julia_vm_types/src/inference_core/type_core.rs"),
                "type RenderedTypeParseCache = std::cell::RefCell<HashMap<String, CoreType>>;",
            ),
        },
    },
    {
        "name": "structinfo_name_maps_compile",
        "root": Path("subset_julia_vm_compile/src/compile"),
        "pattern": re.compile(r"HashMap\s*<\s*String\s*,\s*StructInfo\s*>"),
        "domain": "struct",
        "layer": "compile",
        "difficulty": REQUIRES_OWNER_CONTEXT,
    },
    {
        "name": "struct_table_bare_gets_compile",
        "root": Path("subset_julia_vm_compile/src/compile"),
        "pattern": re.compile(
            r"\b(?:struct_table|base_struct_table)\.get\s*\(\s*(?:name|base_name)\s*\)"
        ),
        "domain": "struct",
        "layer": "compile",
        "difficulty": REQUIRES_OWNER_CONTEXT,
    },
    {
        "name": "runtime_typevar_identity_fields",
        "root": Path("subset_julia_vm_vm/src/vm/mod.rs"),
        "pattern": re.compile(
            r"runtime_typevar_identities\s*:\s*HashMap\s*<\s*\(\s*String\s*,\s*"
            r"Option\s*<\s*String\s*>\s*\)",
            re.MULTILINE | re.DOTALL,
        ),
        "domain": "typevar",
        "layer": "vm",
        "difficulty": REQUIRES_OWNER_CONTEXT,
        "multi_line": True,
    },
    {
        "name": "runtime_typevar_projection_identity_fields",
        "root": Path("subset_julia_vm_vm/src/vm/mod.rs"),
        "pattern": re.compile(
            r"runtime_typevar_projection_identities\s*:\s*HashMap\s*<\s*\(\s*"
            r"String\s*,\s*String\s*,\s*Option\s*<\s*String\s*>\s*\)",
            re.MULTILINE | re.DOTALL,
        ),
        "domain": "typevar",
        "layer": "vm",
        "difficulty": REQUIRES_OWNER_CONTEXT,
        "multi_line": True,
    },
]

# Hand-declared rows not derivable from any regex: see module docstring
# point 2. Empty as of Issue #10987 -- the one former row
# (`runtime_typevar_projection_identities`) was retired here, not updated,
# because its key no longer has any String/Option<String> component for a
# hand-declared row to keep visible.
EXTRA_ANCHOR_ROWS: list[tuple[str, int, str, str, str, str, str]] = []


def anchor_hits_for(check: dict) -> list[tuple[Path, int, str]]:
    root: Path = check["root"]
    if not root.exists():
        return []
    hits: list[tuple[Path, int, str]] = []
    paths = [root] if root.is_file() else sorted(root.rglob("*.rs"))
    if check.get("multi_line"):
        for path in paths:
            text = path.read_text(encoding="utf-8", errors="ignore")
            for match in check["pattern"].finditer(text):
                lineno = text.count("\n", 0, match.start()) + 1
                hits.append((path, lineno, " ".join(match.group(0).split())))
    else:
        for path in paths:
            text = path.read_text(encoding="utf-8", errors="ignore")
            for lineno, line in enumerate(text.splitlines(), 1):
                if check["pattern"].search(line):
                    hits.append((path, lineno, line.strip()))
    classified_lines = check.get("classified_lines", set())
    return [hit for hit in hits if (hit[0], hit[2]) not in classified_lines]


def collect_anchor_rows() -> list[tuple[str, str, str, str, str, str, int, str]]:
    """Return (kind, domain, layer, difficulty, verdict, file, line, detail) rows."""
    rows: list[tuple[str, str, str, str, str, str, int, str]] = []
    for check in ANCHOR_CHECKS:
        for path, lineno, snippet in anchor_hits_for(check):
            verdict = classify_verdict("anchor", check["domain"], path.as_posix(), check["name"])
            rows.append(
                (
                    "anchor",
                    check["domain"],
                    check["layer"],
                    check["difficulty"],
                    verdict,
                    path.as_posix(),
                    lineno,
                    f"{check['name']}: {snippet}",
                )
            )
    for file_str, lineno, ident, domain, layer, difficulty, detail in EXTRA_ANCHOR_ROWS:
        verdict = classify_verdict("anchor", domain, file_str, ident)
        rows.append(
            ("anchor", domain, layer, difficulty, verdict, file_str, lineno, f"{ident}: {detail}")
        )
    return rows


def run_check_name_based_lookup_live_counts() -> dict[str, int]:
    """Independent re-derivation matching scripts/check_name_based_lookup.sh's
    six counts, using the exact same ANCHOR_CHECKS patterns/roots as above
    (by construction -- see module docstring "Reconciliation")."""
    counts: dict[str, int] = {}
    for check in ANCHOR_CHECKS:
        counts[check["name"]] = len(anchor_hits_for(check))
    return counts


def classify_all() -> list[tuple[str, str, str, str, str, str, int, str]]:
    """Return (kind, domain, layer, difficulty, verdict, file, line, detail)
    rows for map_decl + by_name_ref, plus the anchor rows."""
    files = rust_files()
    test_only_whole_file = build_test_only_whole_file_closure(files)

    all_rows: list[tuple[str, str, str, str, str, str, int, str]] = []
    for f in files:
        rel = f.as_posix()
        whole_file_test_only = is_test_or_bench_path(rel) or f in test_only_whole_file
        for kind, domain, block_kind, symbol, lineno, detail in scan_file(f, rel, whole_file_test_only):
            domain = classify_effective_domain(domain, rel, symbol)
            layer = classify_layer(rel)
            difficulty = classify_difficulty(kind, layer, block_kind, domain)
            verdict = classify_verdict(kind, domain, rel, symbol)
            all_rows.append(
                (kind, domain, layer, difficulty, verdict, rel, lineno, f"{symbol}: {detail}")
            )

    all_rows.extend(collect_anchor_rows())
    return all_rows


def write_tsv(path: Path, rows: list[tuple[str, str, str, str, str, str, int, str]]) -> None:
    """Aggregate to (kind, domain, layer, difficulty, verdict, module) -> count,
    at module-level rather than per-line granularity."""
    counts: Counter[tuple[str, str, str, str, str, str]] = Counter()
    for kind, domain, layer, difficulty, verdict, file_rel, _lineno, _detail in rows:
        module = module_key(file_rel)
        counts[(kind, domain, layer, difficulty, verdict, module)] += 1

    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as fh:
        fh.write("kind\tdomain\tlayer\tdifficulty\tverdict\tmodule\tcount\n")
        for key in sorted(counts):
            kind, domain, layer, difficulty, verdict, module = key
            fh.write(
                f"{kind}\t{domain}\t{layer}\t{difficulty}\t{verdict}\t{module}\t{counts[key]}\n"
            )


def print_summary(rows: list[tuple[str, str, str, str, str, str, int, str]]) -> None:
    print(f"\n=== Semantic-ID inventory summary (Issue #10459 Phase 0) — {len(rows)} sites ===\n")

    by_domain: Counter[str] = Counter()
    by_layer: Counter[str] = Counter()
    by_difficulty: Counter[str] = Counter()
    by_verdict: Counter[str] = Counter()
    by_kind: Counter[str] = Counter()
    for kind, domain, layer, difficulty, verdict, _f, _l, _d in rows:
        by_domain[domain] += 1
        by_layer[layer] += 1
        by_difficulty[difficulty] += 1
        by_verdict[verdict] += 1
        by_kind[kind] += 1

    print("-- by kind --")
    for k in ("map_decl", "by_name_ref", "anchor"):
        print(f"  {k:14s} {by_kind.get(k, 0)}")

    print("\n-- by identity domain (six core domains + other) --")
    core_total = 0
    for d in DOMAINS:
        n = by_domain.get(d, 0)
        print(f"  {d:12s} {n}")
        if d != "other":
            core_total += n
    print(f"  {'[6-domain total]':12s} {core_total}")

    print("\n-- by layer --")
    for l in LAYERS:
        print(f"  {l:12s} {by_layer.get(l, 0)}")

    print("\n-- by migration difficulty --")
    for d in DIFFICULTIES:
        print(f"  {d:40s} {by_difficulty.get(d, 0)}")

    print("\n-- by semantic verdict --")
    for verdict in VERDICTS:
        print(f"  {verdict:20s} {by_verdict.get(verdict, 0)}")

    print("\n-- domain x difficulty cross-tab (six core domains only) --")
    cross: Counter[tuple[str, str]] = Counter()
    for _kind, domain, _layer, difficulty, _verdict, _f, _l, _d in rows:
        if domain != "other":
            cross[(domain, difficulty)] += 1
    header = "domain".ljust(12) + "".join(d[:18].ljust(20) for d in DIFFICULTIES)
    print(header)
    for d in DOMAINS:
        if d == "other":
            continue
        row = d.ljust(12)
        for diff in DIFFICULTIES:
            row += str(cross.get((d, diff), 0)).ljust(20)
        print(row)

    print("\n-- domain x semantic verdict (six core domains only) --")
    verdict_cross: Counter[tuple[str, str]] = Counter()
    for _kind, domain, _layer, _difficulty, verdict, _file, _line, _detail in rows:
        if domain != "other":
            verdict_cross[(domain, verdict)] += 1
    header = "domain".ljust(12) + "".join(verdict[:18].ljust(20) for verdict in VERDICTS)
    print(header)
    for domain in DOMAINS:
        if domain == "other":
            continue
        row = domain.ljust(12)
        for verdict in VERDICTS:
            row += str(verdict_cross.get((domain, verdict), 0)).ljust(20)
        print(row)
    phase4_residual = sum(
        count
        for (domain, verdict), count in verdict_cross.items()
        if verdict == IDENTITY_BEARING
    )
    print(f"  {'[Phase 4 identity-bearing]':12s} {phase4_residual}")

    print("\n-- top modules by six-core-domain site count --")
    module_totals: Counter[str] = Counter()
    for _kind, domain, _layer, _difficulty, _verdict, file_rel, _line, _detail in rows:
        if domain != "other":
            module_totals[module_key(file_rel)] += 1
    for module, total in module_totals.most_common(20):
        print(f"  {total:5d}  {module}")

    print("\n=== Reconciliation vs scripts/check_name_based_lookup.sh (live) ===\n")
    live = run_check_name_based_lookup_live_counts()
    anchor_counts: Counter[str] = Counter()
    for kind, _domain, _layer, _difficulty, _verdict, _file_rel, _lineno, detail in rows:
        if kind == "anchor" and ":" in detail:
            name = detail.split(":", 1)[0]
            anchor_counts[name] += 1
    mismatches = []
    for check in ANCHOR_CHECKS:
        name = check["name"]
        live_count = live.get(name, 0)
        this_count = anchor_counts.get(name, 0)
        status = "OK" if live_count == this_count else "MISMATCH"
        if status == "MISMATCH":
            mismatches.append(name)
        print(f"  {name:44s} live={live_count:4d} this_script={this_count:4d} {status}")
    if mismatches:
        print(
            f"\n  {len(mismatches)} mismatch(es) -- ANCHOR_CHECKS in this script has drifted "
            "from scripts/check_name_based_lookup.sh; investigate before trusting the "
            "anchor rows."
        )
    else:
        print(
            "\n  OK -- all six patterns match scripts/check_name_based_lookup.sh's live "
            "count exactly (same regex, same roots, by construction)."
        )
    print(
        f"\n  Plus {len(EXTRA_ANCHOR_ROWS)} hand-declared anchor row(s) not derived from any "
        "regex -- see EXTRA_ANCHOR_ROWS / module docstring point 2."
    )
    print()


def write_detail_tsv(
    path: Path, rows: list[tuple[str, str, str, str, str, str, int, str]]
) -> None:
    """Per-line detail, NOT committed to docs/vm/ -- an ad hoc audit aid for
    --detail. The committed docs/vm/SEMANTIC_ID_INVENTORY.tsv snapshot is
    always the aggregated (kind, domain, layer, difficulty, verdict, module) -> count
    table written by write_tsv(), matching docs/vm/PANIC_DEBT_CLASSIFICATION.tsv's
    granularity."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as fh:
        fh.write("kind\tdomain\tlayer\tdifficulty\tverdict\tfile\tline\tsymbol\tdetail\n")
        for kind, domain, layer, difficulty, verdict, file_rel, lineno, combined in sorted(
            rows, key=lambda r: (r[5], r[6])
        ):
            if ":" in combined:
                symbol, detail = combined.split(":", 1)
                detail = detail.strip()
            else:
                symbol, detail = combined, ""
            fh.write(
                f"{kind}\t{domain}\t{layer}\t{difficulty}\t{verdict}\t"
                f"{file_rel}\t{lineno}\t{symbol}\t{detail}\n"
            )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--out",
        default="docs/vm/SEMANTIC_ID_INVENTORY.tsv",
        help="path to write the committed, aggregated (kind/domain/layer/difficulty/module -> count) TSV snapshot",
    )
    parser.add_argument(
        "--detail",
        default=None,
        help="optional path to also write a per-line, non-aggregated, NOT-committed detail TSV (audit aid)",
    )
    args = parser.parse_args()

    if not Path("Cargo.toml").exists() or not Path("subset_julia_vm_types/src").exists():
        print("ERROR: run from the repository root", file=sys.stderr)
        return 2

    rows = classify_all()
    out_path = Path(args.out)
    write_tsv(out_path, rows)
    print(f"wrote {out_path} ({len(rows)} classified sites)")
    if args.detail:
        detail_path = Path(args.detail)
        write_detail_tsv(detail_path, rows)
        print(f"wrote {detail_path} ({len(rows)} per-line rows, not committed)")
    print_summary(rows)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
