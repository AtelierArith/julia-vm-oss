#!/usr/bin/env python3
"""Classify panic-prone static sites into Issue #10869 Phase 0 buckets.

This is a REPORT GENERATOR, not a CI gate. It never fails the build and is
not wired into any check_*.sh / premerge_gate.sh flow. Re-run it any time to
regenerate docs/vm/PANIC_DEBT_CLASSIFICATION.tsv; there is no ratchet on its
output.

Issue #10869 asks Phase 0 to classify every static `unwrap_call` / `expect_call`
/ `panic_macro` (plus `todo_macro` / `unimplemented_macro`, tracked for parity
with scripts/panic_free_inventory.py) site into exactly one of four buckets:

  - test-only              — compiled only under `#[cfg(test)]` / lives in a
                              tests/benches path.
  - build-time invariant   — runs during `cargo build`, a dev-only tool
                              invoked outside the shipped CLI/REPL/FFI/Web
                              surface, or parses fixed/trusted bundled source
                              (not external user input).
  - cache-corruption boundary — (de)serializes a persisted or embedded binary
                              cache/bytecode payload that could be stale,
                              truncated, or produced by a different sjulia
                              version.
  - user-input reachable  — everything else: parser, lowering, compile front
                              door, VM (incl. specialize/type_ops/formatting/
                              register VM), REPL/session, AoT, FFI, Web,
                              macro expansion, the `subset_julia_vm_runtime`
                              AoT support crate, and the CLI binaries. This is
                              the bucket Issue #10869 is retiring debt from.

## Mechanism (mechanical, re-runnable; no hand-authored per-site table)

1. **Scope**: the same five crate `src/` roots plus `build.rs` files used by
   `scripts/check_panic_free_ratchet.sh` (`PANIC_RATCHET_ROOTS`), so grand
   totals reconcile against `docs/vm/PANIC_FREE_RATCHET_BASELINE.tsv` and
   `scripts/panic_free_inventory.sh --skip-clippy` by construction (same
   files, same five regexes).

2. **`module_key()`** is copied verbatim from
   `scripts/check_panic_free_ratchet.sh` so this script's per-module rows are
   directly comparable to the committed ratchet baseline (see the
   `--reconcile` output).

3. **test-only** is derived mechanically, with no per-file judgment calls:
   - path pattern: `/tests/`, `/benches/`, or a `..._tests.rs` filename
     (identical to `scripts/panic_free_inventory.py`'s `classify_path`
     `test_or_bench` branch);
   - cross-file closure: every `#[cfg(test)] mod ident;` (or `#[cfg(any(...,
     test, ...))] mod ident;`) declaration is resolved to its child file
     (`<dir>/ident.rs` or `<dir>/ident/mod.rs`, using the same file-vs-`mod.rs`
     resolution rule rustc uses) and the whole subtree reachable from that
     child (following further `mod ident;` declarations inside it,
     regardless of their own attributes — once a subtree's root is
     compiled only under `cfg(test)`, everything under it is too) is
     test-only. This matters in practice: e.g. `subset_julia_vm_vm/src/vm/mod.rs`
     declares `#[cfg(test)] mod tests;`, so `vm/tests.rs`'s ~87 `.unwrap()` /
     32 `.expect()` sites are test-only even though the filename `tests.rs`
     does not end in `_tests.rs` and would otherwise be missed by the path
     pattern above;
   - inline scope: a brace-depth scan (over a string/char/raw-string/
     comment-masked copy of the file, so `"{"` inside a Julia sample string
     literal never perturbs the count) marks any line inside a
     `#[cfg(test)]` / `#[cfg(any(..., test, ...))]` / `#[test]`-gated block
     as test-only, e.g. `subset_julia_vm_parser/src/cst.rs`'s inline
     `#[cfg(any(test, feature = "testing"))] pub mod testing { ... }` (which
     legitimately panics on assertion failure — see the module's own doc
     comment).
   Known heuristic gaps (documented, not fixed): `cfg(not(test))` would be
   (mis)treated as test-gated by a naive "contains `test`" check — guarded
   against explicitly since it does not occur in this repo today (see
   `CFG_TEST_RE`); multi-line raw strings are stripped file-wide before line
   splitting so they cannot corrupt brace counts, but an attribute split
   across a macro-generated `include!`/`macro_rules!` expansion is invisible
   to a source-text scan (accepted: mechanical source scan, not a compiler).

4. **build-time invariant** and **cache-corruption boundary** are an
   explicit, reviewer-auditable `RULES_BY_FILE` table below (exact file path
   -> (bucket, one-line reason)) — this is the "judgment call" surface the
   issue asks to make auditable rather than mechanical. Every row cites the
   concrete reason (dev-only tool, build script, or the specific persisted
   cache format it (de)serializes). Anything not in `RULES_BY_FILE` and not
   test-only defaults to **user-input reachable** — the conservative
   direction: an unrecognized file is assumed to be on a user-reachable path
   rather than silently exempted.

## Reconciliation

`main()` prints:
  - grand totals per metric, compared against the Issue #10869 evidence
    snapshot (origin/main `92a77484`, 2026-07-13: unwrap=1,215 expect=740
    panic=256) and against a fresh run of this script. A small drift in
    `unwrap_call` is expected and NOT a bug: ordinary commits land on `main`
    between the issue's evidence snapshot and any later run of this script
    (Issue #10870 resynced the ratchet baseline the same day). Investigate
    only if the drift is large or `expect_call`/`panic_macro` move, since
    those two counts matched exactly at the time this script was written.
  - a per-(metric, module) reconciliation against
    `docs/vm/PANIC_FREE_RATCHET_BASELINE.tsv`: since `module_key()` is copied
    verbatim and the file scope is identical, the sum across buckets for a
    given (metric, module) must equal that file's committed baseline row
    exactly (the ratchet was confirmed green — i.e. baseline == current — by
    Issue #10870 immediately before this script was written). Any mismatch
    printed here means the two scripts have drifted and should be
    investigated before trusting the classification.

Usage:
    python3 scripts/panic_debt_classification.py
    python3 scripts/panic_debt_classification.py --out docs/vm/PANIC_DEBT_CLASSIFICATION.tsv
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter, defaultdict, deque
from pathlib import Path

METRIC_PATTERNS: dict[str, re.Pattern[str]] = {
    "unwrap_call": re.compile(r"\.unwrap\s*\("),
    "expect_call": re.compile(r"\.expect\s*\("),
    "panic_macro": re.compile(r"(?<![A-Za-z0-9_])panic!\s*\("),
    "todo_macro": re.compile(r"(?<![A-Za-z0-9_])todo!\s*\("),
    "unimplemented_macro": re.compile(r"(?<![A-Za-z0-9_])unimplemented!\s*\("),
}

# Identical scope to scripts/check_panic_free_ratchet.sh's PANIC_RATCHET_ROOTS
# default, so grand totals reconcile by construction.
ROOT_DIRS = (
    Path("subset_julia_vm/src"),
    Path("subset_julia_vm_ffi/src"),
    Path("subset_julia_vm_parser/src"),
    Path("subset_julia_vm_runtime/src"),
    Path("subset_julia_vm_web/src"),
)
ROOT_FILES = (Path("build.rs"), Path("subset_julia_vm/build.rs"))

BASELINE_TSV = Path("docs/vm/PANIC_FREE_RATCHET_BASELINE.tsv")
EVIDENCE_SNAPSHOT = {"unwrap_call": 1215, "expect_call": 740, "panic_macro": 256}


# ---------------------------------------------------------------------------
# module_key(): copied verbatim from scripts/check_panic_free_ratchet.sh so
# this script's module column lines up 1:1 with
# docs/vm/PANIC_FREE_RATCHET_BASELINE.tsv rows.
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


def is_test_or_bench_path(rel: str) -> bool:
    """Same predicate as panic_free_inventory.py's classify_path test_or_bench branch."""
    p = rel.replace("\\", "/")
    return "/tests/" in p or p.endswith("_tests.rs") or "/benches/" in p


# ---------------------------------------------------------------------------
# Bucket rule table (Issue #10869 Phase 0 "explicit, commented rule table").
#
# Exact file path (relative to repo root) -> (bucket, one-line reason).
# First lookup wins; anything absent defaults to "user-input-reachable".
# Ordering has no effect (dict is keyed by exact path, not prefix), but rows
# are grouped by bucket for readability.
# ---------------------------------------------------------------------------
BUILD_TIME_INVARIANT = "build-time-invariant"
CACHE_CORRUPTION_BOUNDARY = "cache-corruption-boundary"
USER_INPUT_REACHABLE = "user-input-reachable"
TEST_ONLY = "test-only"

RULES_BY_FILE: dict[str, tuple[str, str]] = {
    # --- build-time invariant ---------------------------------------------
    "build.rs": (
        BUILD_TIME_INVARIANT,
        "workspace root build script; runs during `cargo build`, before any user Julia source exists",
    ),
    "subset_julia_vm/build.rs": (
        BUILD_TIME_INVARIANT,
        "crate build script (Base cache embedding / schema fingerprint generation); build-time only",
    ),
    "subset_julia_vm/src/base_loader.rs": (
        BUILD_TIME_INVARIANT,
        "parses the bundled trusted Base Julia source at process startup — a panic here is an sjulia "
        "Base-implementation bug, not an externally reachable input path (judgment call, same trust "
        "class as build.rs; see docs/vm/PANIC_DEBT_RETIREMENT.md)",
    ),
    "subset_julia_vm/src/stdlib_loader.rs": (
        BUILD_TIME_INVARIANT,
        "parses the bundled trusted stdlib Julia source at process startup; same rationale as base_loader.rs",
    ),
    "subset_julia_vm/src/bin/compile_samples.rs": (
        BUILD_TIME_INVARIANT,
        "dev generator (`cargo run --bin compile_samples`) over a hardcoded Vec of sample Julia source "
        "embedded in this file; produces web/samples_ir.js, not reachable from the shipped CLI/REPL/FFI/Web surface",
    ),
    "subset_julia_vm/src/bin/dispatch_inline_cache_bench_8561.rs": (
        BUILD_TIME_INVARIANT,
        "perf-measurement dev harness (Issue #8561) over fixed embedded benchmark Julia source; not a shipped entrypoint",
    ),
    "subset_julia_vm/src/bin/handler_table_bench_8562.rs": (
        BUILD_TIME_INVARIANT,
        "perf-measurement dev harness (Issue #8562) over fixed embedded benchmark Julia source; not a shipped entrypoint",
    ),
    "subset_julia_vm/src/bin/register_vm_bench_8559.rs": (
        BUILD_TIME_INVARIANT,
        "perf-measurement dev harness (Issue #8559) over fixed embedded benchmark Julia source; not a shipped entrypoint",
    ),
    "subset_julia_vm_parser/src/bin/parse_corpus.rs": (
        BUILD_TIME_INVARIANT,
        "corpus differential-testing dev harness driven by scripts/parser_corpus_sweep.sh; not a shipped entrypoint",
    ),
    # --- cache-corruption boundary -----------------------------------------
    "subset_julia_vm_compile/src/compile/cache.rs": (
        CACHE_CORRUPTION_BOUNDARY,
        "thread-local PROGRAM_CACHE / Base function cache keyed by content hash — the 'cache load' entrypoint named in Issue #10869",
    ),
    "subset_julia_vm_compile/src/compile/embedded_cache.rs": (
        CACHE_CORRUPTION_BOUNDARY,
        "`include_bytes!`-embedded precompiled Base cache decode path (SJULIA_BASE_CACHE)",
    ),
    "subset_julia_vm_compile/src/compile/preload_cache.rs": (
        CACHE_CORRUPTION_BOUNDARY,
        "preloaded-package bytecode cache (Issue #9189); deserializes persisted package bytecode",
    ),
    "subset_julia_vm_compile/src/compile/seeded_cache.rs": (
        CACHE_CORRUPTION_BOUNDARY,
        "seeded PROGRAM_CACHE entries (Issue #10120); persisted/precomputed CompiledProgram entries",
    ),
    "subset_julia_vm_compile/src/compile/precompile.rs": (
        CACHE_CORRUPTION_BOUNDARY,
        "serialize_base_cache / deserialize_base_cache (docs/vm/CACHE_ARCHITECTURE.md) — the persisted Base cache (de)serialization boundary",
    ),
    "subset_julia_vm/src/core_ir_file.rs": (
        CACHE_CORRUPTION_BOUNDARY,
        "`.sjir` persisted Core IR file format load/save",
    ),
    "subset_julia_vm/src/vm_bytecode_file.rs": (
        CACHE_CORRUPTION_BOUNDARY,
        "`.sjvmbc` persisted VM bytecode file format load/save (Issue #10170 invalidation contract)",
    ),
    "subset_julia_vm/src/loader.rs": (
        CACHE_CORRUPTION_BOUNDARY,
        "package loader's persistent `.ji.json` cache (source-hash + schema-fingerprint keyed); same invalidate-on-mismatch contract as the other cache files",
    ),
    "subset_julia_vm_ffi/src/bytecode.rs": (
        CACHE_CORRUPTION_BOUNDARY,
        "`.sjvmbc` FFI load path; per-file doc comment requires any load failure be treated as a cache miss, never surfaced to the user",
    ),
}


# ---------------------------------------------------------------------------
# Source masking: replace string/char/raw-string/comment contents with 'x' so
# a Julia sample string like `"Array{Float64}"` embedded in Rust source never
# perturbs brace-depth counting. Newlines are preserved so line numbers stay
# aligned with the original file.
# ---------------------------------------------------------------------------
# `(?<![A-Za-z0-9_])` requires the `r`/`b` raw-string prefix to start a fresh
# token, NOT the tail of a preceding word — without it, an ordinary string
# ending in a word that ends in "r" (e.g. `"parse/lower"`, `"...compiler..."`)
# is misread as the START of a raw string, swallowing everything up to the
# next unrelated `"` in the file. Caught by a before/after-cfg(test) spot
# check on subset_julia_vm_compile/src/compile/preload_cache.rs during authoring
# (Issue #10869 Phase 0): line 815's `.expect("parse/lower")` was
# misidentified as opening `r"..."` at its trailing "r\"", swallowing lines
# 816-834 and corrupting the brace count used for inline test-scope
# detection. `b?` additionally covers raw *byte* strings (`br"..."`).
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


# `cfg(test)`, `cfg(any(test, ...))`, `cfg(all(..., test, ...))` all count as
# test-gated. `cfg(not(test))` (inverted meaning) does not occur in this repo
# today (checked at authoring time); guarded explicitly rather than silently
# mismatched.
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
    """Return (child_module_ident, is_cfg_test_gated) for every `mod ident;`."""
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
    """Per-line: is this line inside a #[cfg(test)]/#[test]-gated brace scope?"""
    depth = 0
    scope_entry_depths: list[int] = []
    pending_test_attr = False
    result: list[bool] = []

    for line in masked_lines:
        if is_test_cfg_attr(line):
            # Covers both a standalone `#[cfg(test)]` line (the brace that
            # opens the gated block arrives on a later line) and a combined
            # `#[cfg(test)] mod tests {` on one line (the brace scan below
            # sees pending_test_attr already set before it reaches that `{`).
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
    for extra in ROOT_FILES:
        if extra.exists():
            files.append(extra)
    return sorted(set(files))


def classify(files: list[Path]) -> tuple[Counter[tuple[str, str, str]], Counter[tuple[str, str]]]:
    """Return (bucket, metric, module) -> count, and (metric, module) -> total count."""
    test_only_whole_file = build_test_only_whole_file_closure(files)

    bucket_counts: Counter[tuple[str, str, str]] = Counter()
    module_metric_totals: Counter[tuple[str, str]] = Counter()

    for f in files:
        rel = f.as_posix()
        try:
            text = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        lines = text.splitlines()
        module = module_key(rel)

        whole_file_test_only = is_test_or_bench_path(rel) or f in test_only_whole_file
        if whole_file_test_only:
            in_test = [True] * len(lines)
        else:
            masked_lines = mask_non_code(text).splitlines()
            if len(masked_lines) != len(lines):
                # Masking must preserve line count; fall back to "not test"
                # rather than mis-index if a pathological file breaks that
                # invariant (defensive only — not expected in practice).
                in_test = [False] * len(lines)
            else:
                in_test = inline_test_scope_lines(masked_lines)

        file_rule = RULES_BY_FILE.get(rel)

        for metric, pattern in METRIC_PATTERNS.items():
            for line_idx, line in enumerate(lines):
                n = len(pattern.findall(line))
                if n == 0:
                    continue
                module_metric_totals[(metric, module)] += n
                if whole_file_test_only or (line_idx < len(in_test) and in_test[line_idx]):
                    bucket = TEST_ONLY
                elif file_rule is not None:
                    bucket = file_rule[0]
                else:
                    bucket = USER_INPUT_REACHABLE
                bucket_counts[(bucket, metric, module)] += n

    return bucket_counts, module_metric_totals


def find_brace_imbalanced_files(files: list[Path]) -> list[tuple[int, str]]:
    """Diagnostic (Issue #10869 Phase 0): a syntactically valid Rust file's
    masked (string/char/comment-stripped) text must have equal `{`/`}`
    counts. A nonzero count flags a residual mask_non_code() gap (the
    Phase 0 authoring session found and fixed one such class of bug — an
    unanchored raw-string prefix — via exactly this check; see
    `_RAW_STRING_RE`'s comment). Informational only: every flagged file at
    authoring time was manually confirmed to still classify its actual
    unwrap/expect/panic! lines correctly (the imbalance fell in a stretch of
    the file with no such lines), so this does not gate the script — but a
    future run that adds NEW flagged files should double-check them before
    trusting their bucket assignment.
    """
    imbalanced = []
    for f in files:
        text = f.read_text(encoding="utf-8", errors="ignore")
        masked = mask_non_code(text)
        depth = masked.count("{") - masked.count("}")
        if depth != 0:
            imbalanced.append((depth, f.as_posix()))
    return sorted(imbalanced)


def write_tsv(path: Path, counts: Counter[tuple[str, str, str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as f:
        f.write("bucket\tmetric\tmodule\tcount\n")
        for bucket, metric, module in sorted(counts):
            f.write(f"{bucket}\t{metric}\t{module}\t{counts[(bucket, metric, module)]}\n")


def parse_ratchet_baseline(path: Path) -> dict[tuple[str, str], int]:
    baseline: dict[tuple[str, str], int] = {}
    if not path.exists():
        return baseline
    for line in path.read_text(encoding="utf-8").splitlines()[1:]:
        if not line.strip():
            continue
        metric, module, count = line.split("\t")
        baseline[(metric, module)] = int(count)
    return baseline


def print_summary(
    bucket_counts: Counter[tuple[str, str, str]],
    module_metric_totals: Counter[tuple[str, str]],
    brace_imbalanced: list[tuple[int, str]],
) -> None:
    metrics = sorted(METRIC_PATTERNS)
    buckets = [TEST_ONLY, BUILD_TIME_INVARIANT, CACHE_CORRUPTION_BOUNDARY, USER_INPUT_REACHABLE]

    print("\n=== Panic-debt classification summary (Issue #10869 Phase 0) ===\n")
    header = "bucket".ljust(28) + "".join(m.ljust(22) for m in metrics)
    print(header)
    grand_total: Counter[str] = Counter()
    for bucket in buckets:
        row = bucket.ljust(28)
        for metric in metrics:
            total = sum(c for (b, m, _mod), c in bucket_counts.items() if b == bucket and m == metric)
            grand_total[metric] += total
            row += str(total).ljust(22)
        print(row)
    print("-" * (28 + 22 * len(metrics)))
    total_row = "TOTAL".ljust(28)
    for metric in metrics:
        total_row += str(grand_total[metric]).ljust(22)
    print(total_row)

    print("\n=== Reconciliation vs Issue #10869 evidence snapshot (origin/main 92a77484) ===\n")
    for metric, evidence in EVIDENCE_SNAPSHOT.items():
        current = grand_total[metric]
        delta = current - evidence
        note = "OK (exact match)" if delta == 0 else f"drift={delta:+d} (see script header: expected for unwrap_call)"
        print(f"  {metric}: evidence={evidence} current={current} {note}")

    print("\n=== Per-module reconciliation vs docs/vm/PANIC_FREE_RATCHET_BASELINE.tsv ===\n")
    baseline = parse_ratchet_baseline(BASELINE_TSV)
    mismatches = []
    for key in sorted(set(baseline) | set(module_metric_totals)):
        base = baseline.get(key, 0)
        current = module_metric_totals.get(key, 0)
        if base != current:
            mismatches.append((key, base, current))
    if mismatches:
        print(f"  {len(mismatches)} mismatch(es) (investigate before trusting the classification):")
        for (metric, module), base, current in mismatches:
            print(f"    {metric}\t{module}: baseline={base} current={current}")
    else:
        print(f"  OK — all {len(baseline)} (metric, module) rows match the committed ratchet baseline exactly.")

    print("\n=== user-input-reachable bucket: top modules by unwrap+expect+panic ===\n")
    reachable_module_totals: Counter[str] = Counter()
    for (bucket, metric, module), c in bucket_counts.items():
        if bucket == USER_INPUT_REACHABLE and metric in ("unwrap_call", "expect_call", "panic_macro"):
            reachable_module_totals[module] += c
    for module, total in reachable_module_totals.most_common(25):
        print(f"  {total:6d}  {module}")
    print()

    print("=== mask_non_code() diagnostic: files with unbalanced masked braces ===\n")
    if brace_imbalanced:
        print(f"  {len(brace_imbalanced)} file(s) — informational, see find_brace_imbalanced_files() docstring:")
        for depth, name in brace_imbalanced:
            print(f"    {depth:+d}  {name}")
    else:
        print("  none — every scanned file's masked brace count is balanced.")
    print()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--out",
        default="docs/vm/PANIC_DEBT_CLASSIFICATION.tsv",
        help="path to write the committed TSV snapshot",
    )
    args = parser.parse_args()

    if not Path("Cargo.toml").exists() or not Path("subset_julia_vm_ffi/src").exists():
        print("ERROR: run from the repository root", file=sys.stderr)
        return 2

    files = rust_files()
    bucket_counts, module_metric_totals = classify(files)
    brace_imbalanced = find_brace_imbalanced_files(files)
    out_path = Path(args.out)
    write_tsv(out_path, bucket_counts)
    print(f"wrote {out_path} ({sum(bucket_counts.values())} classified hits across {len(files)} files)")
    print_summary(bucket_counts, module_metric_totals, brace_imbalanced)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
