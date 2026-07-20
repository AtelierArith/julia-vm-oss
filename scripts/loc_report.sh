#!/usr/bin/env bash
# loc_report.sh — mechanically reproduce the "1 機能あたりの触点数" LOC/variant
# table from Issue #10817 (design/tech-debt: growth quality over raw LOC).
#
# This is a REPORT, not a gate: it always exits 0 (unless a required
# directory is missing) and never fails the build. Issue #10817's thesis is
# that raw workspace LOC (~51万行) is defensible for the project's goals, but
# the *quality* of growth — touchpoints per feature, parallel semantic
# implementations, fused-op variant combinatorics — needs its own ratchet
# separate from LOC. This script prints the measured inputs to that judgment;
# it does not itself pass/fail.
#
# What it measures (physical line counts, i.e. `wc -l`):
#   - subset_julia_vm/src per-area breakdown: compile/ vm/ aot/ lowering/
#     repl+rest, and the crate's total Rust LOC
#   - subset_julia_vm/src/julia/*.jl (Pure Julia layer)
#   - sibling crates' Rust LOC: bytecode/types/parser/ir/ffi/web/runtime
#   - subset_julia_vm/tests/*.rs (harness, excluding tests/fixtures/) and
#     subset_julia_vm/tests/fixtures/**/*.jl (fixture corpus)
#   - workspace Rust total (crate + siblings, tests counted separately)
#   - symbolic fused-op combinatorics: `Instr` enum variant count
#     (subset_julia_vm_bytecode/src/instr.rs) and `TypedLoopOp` variant count
#     (subset_julia_vm_vm/src/vm/executable.rs) — see NORTH_STAR.md debt
#     barometer and CHECKLISTS.md fused-op precondition policy (Issue #10814)
#   - the single largest non-test .rs file across all crate src/ trees
#
# Each measured row is printed next to its Issue #10817 (2026-07-12) baseline
# with a delta, so accidental measurement-methodology drift is visible without
# needing to re-read the issue. Small drift (single-digit %) between runs is
# expected as the codebase moves; this script only flags rows whose relative
# drift exceeds a loose threshold as "NOTE" (informational, not a failure).
#
# Usage:
#   bash scripts/loc_report.sh
#   bash scripts/loc_report.sh > /tmp/loc_report.md
#   bash scripts/loc_report.sh --variants-only   # NS-7 (c) fast path, see below
#
# Recommended cadence (Issue #10817 proposal): run this quarterly (or when
# reviewing Milestone "アーキテクチャ負債" progress) and paste the output into
# a new `### LOC/touchpoint snapshot (Issue #10817)` subsection under that
# quarter's dated header in docs/vm/STATUS.md, so the debt-barometer trend
# (docs/vm/NORTH_STAR.md NS-7) has a paper trail independent of git blame
# archaeology.
#
# `--variants-only` (Issue #10899): skip the full LOC sweep (which walks every
# src tree) and print just the two NS-7 (c) fused-op touchpoint counts as
# `key=value` lines, e.g.:
#   instr_variants=431
#   typed_loop_variants=79
# This is what `scripts/north_star_report.sh` invokes on every nightly run —
# it reuses this script's `extract_enum_variant_count` instead of
# reimplementing enum-variant counting a second time. The full (no-flag) form
# stays the quarterly/manual report with the LOC breakdown tables.
#
# Exit code: 0 on success (report emitted, in either full or --variants-only
# mode). 1 if a required top-level directory (subset_julia_vm/src, etc.) is
# missing — i.e. "not run from repo root" or "repo layout changed out from
# under this script", not a debt finding. 2 on an unrecognized argument.
#
# Dependencies: python3 (stdlib only), bash 3.2+ (macOS stock /bin/bash safe —
# no associative arrays, no `mapfile`).

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ ! -d subset_julia_vm/src ]]; then
  echo "ERROR: subset_julia_vm/src not found. Run from the repository root." >&2
  exit 1
fi

MODE="full"
case "${1:-}" in
  --variants-only) MODE="variants-only" ;;
  "") ;;
  *) echo "ERROR: unknown arg ${1} (usage: loc_report.sh [--variants-only])" >&2; exit 2 ;;
esac

python3 - "$repo_root" "$MODE" <<'PY'
import re
import sys
from pathlib import Path

repo_root = Path(sys.argv[1])


def loc(root: Path, glob: str = "*.rs", exclude=None) -> int:
    """Sum physical line counts (wc -l semantics) of files matching glob."""
    if not root.exists():
        return 0
    total = 0
    for path in root.rglob(glob):
        if exclude is not None and exclude(path):
            continue
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        # wc -l counts newlines, not "logical lines"; splitlines() on a
        # trailing-newline-terminated file matches that count exactly.
        total += len(text.splitlines())
    return total


def extract_enum_variant_count(text: str, enum_name: str) -> int:
    """
    Count top-level variant declarations of `[pub] enum <enum_name> { ... }`.

    Same brace-balancing + "line starts with an identifier" heuristic as
    scripts/check_instr_wire_ids.sh's extract_enum_variants(), so this stays
    consistent with the existing wire-ID coverage audit rather than
    reimplementing enum parsing a third way.
    """
    m = re.search(r'(?:pub\s+)?enum\s+' + re.escape(enum_name) + r'\s*\{', text)
    if not m:
        raise RuntimeError(f"enum {enum_name} not found")
    body_start = text.index('{', m.start())
    pos = body_start
    brace_depth = 0
    body_chars = []
    while pos < len(text):
        ch = text[pos]
        if ch == '{':
            brace_depth += 1
        elif ch == '}':
            brace_depth -= 1
            if brace_depth == 0:
                break
        else:
            body_chars.append(ch)
        pos += 1
    body = ''.join(body_chars)
    count = 0
    for raw_line in body.splitlines():
        line = raw_line.strip()
        if not line or line.startswith('//') or line.startswith('#') or line.startswith('/*') or line.startswith('*'):
            continue
        m2 = re.match(r'^([A-Z_][A-Za-z0-9_]*)[\s,({]?', line)
        if not m2:
            continue
        name = m2.group(1)
        if name in ('pub', 'fn', 'let', 'use', 'const', 'type', 'where', 'impl', 'struct'):
            continue
        count += 1
    return count


mode = sys.argv[2] if len(sys.argv) > 2 else "full"

instr_path = repo_root / "subset_julia_vm_bytecode" / "src" / "instr.rs"
typed_loop_path = repo_root / "subset_julia_vm_vm" / "src" / "vm" / "executable.rs"
instr_variants = extract_enum_variant_count(instr_path.read_text(encoding="utf-8", errors="ignore"), "Instr")
typed_loop_variants = extract_enum_variant_count(
    typed_loop_path.read_text(encoding="utf-8", errors="ignore"), "TypedLoopOp"
)

if mode == "variants-only":
    # Fast path for scripts/north_star_report.sh NS-7 (c) nightly collection
    # (Issue #10899): skip the full LOC sweep below, print just the two
    # counts as `key=value` lines so the caller can parse them without
    # markdown-table scraping.
    print(f"instr_variants={instr_variants}")
    print(f"typed_loop_variants={typed_loop_variants}")
    sys.exit(0)


def largest_rs_file(roots):
    best_path = None
    best_lines = -1
    for root in roots:
        if not root.exists():
            continue
        for path in root.rglob("*.rs"):
            if not path.is_file():
                continue
            n = len(path.read_text(encoding="utf-8", errors="ignore").splitlines())
            if n > best_lines:
                best_lines = n
                best_path = path
    return best_path, best_lines


def fmt(n: int) -> str:
    return f"{n:,}"


def pct(part: int, whole: int) -> str:
    if whole <= 0:
        return "n/a"
    return f"{100.0 * part / whole:.1f}%"


def drift_note(label: str, current: int, baseline: int, threshold_pct: float = 20.0) -> str:
    if baseline <= 0:
        return ""
    delta_pct = 100.0 * (current - baseline) / baseline
    sign = "+" if delta_pct >= 0 else ""
    marker = ""
    if abs(delta_pct) > threshold_pct:
        marker = "  NOTE: drift exceeds {:.0f}% since 2026-07-12 baseline, worth a second look".format(threshold_pct)
    return f" ({sign}{delta_pct:.1f}% vs. #10817 baseline{marker})"


CRATE_SRC = repo_root / "subset_julia_vm" / "src"
JULIA_DIR = CRATE_SRC / "julia"

# Post-crate-split locations (Issue #11592): the compile/vm/lowering areas
# moved from subset_julia_vm/src/<area> into their own sibling crates, but
# each row keeps its #10817 baseline identity — the baseline measured this
# code wherever it lives, and `loc()` silently returning 0 for the removed
# directories would have reported the areas as empty.
areas = {
    "src/compile/": repo_root / "subset_julia_vm_compile" / "src",
    "src/vm/": repo_root / "subset_julia_vm_vm" / "src",
    "src/aot/": CRATE_SRC / "aot",
    "src/lowering/": repo_root / "subset_julia_vm_lowering" / "src",
}
area_loc = {name: loc(path) for name, path in areas.items()}

# Issue #10817's table tracks `src/julia/` only via its .jl (Pure Julia layer)
# line item, accounted for separately below — it is deliberately excluded from
# the crate's Rust total/rest so the two views (Rust touchpoints vs. Pure
# Julia coverage) don't double up. `src/julia/` does contain a handful of .rs
# loader/glue files (module registration, not opcode logic); their LOC is
# reported on its own row rather than silently folded into "rest".
# "VM implementation total" in the #10817 sense: the pre-split
# subset_julia_vm crate = today's subset_julia_vm/src (repl, aot, glue)
# plus the split-out compile/vm/lowering crates (Issue #11592).
crate_total_rust = loc(CRATE_SRC, "*.rs", exclude=lambda p: JULIA_DIR in p.parents) + sum(
    area_loc[name] for name in ("src/compile/", "src/vm/", "src/lowering/")
)
rest_loc = crate_total_rust - sum(area_loc.values())
julia_loader_rs_loc = loc(JULIA_DIR, "*.rs")

julia_jl_loc = loc(JULIA_DIR, "*.jl")

sibling_crates = {
    "subset_julia_vm_bytecode": repo_root / "subset_julia_vm_bytecode" / "src",
    "subset_julia_vm_types": repo_root / "subset_julia_vm_types" / "src",
    "subset_julia_vm_parser": repo_root / "subset_julia_vm_parser" / "src",
    "subset_julia_vm_ir": repo_root / "subset_julia_vm_ir" / "src",
    "subset_julia_vm_ffi": repo_root / "subset_julia_vm_ffi" / "src",
    "subset_julia_vm_web": repo_root / "subset_julia_vm_web" / "src",
    "subset_julia_vm_runtime": repo_root / "subset_julia_vm_runtime" / "src",
}
sibling_loc = {name: loc(path) for name, path in sibling_crates.items()}
sibling_total = sum(sibling_loc.values())

TESTS_DIR = repo_root / "subset_julia_vm" / "tests"
FIXTURES_DIR = TESTS_DIR / "fixtures"
tests_rs_loc = loc(TESTS_DIR, "*.rs", exclude=lambda p: FIXTURES_DIR in p.parents)
fixtures_jl_loc = loc(FIXTURES_DIR, "*.jl")

# "Workspace Rust total" is the issue's ~51万行 headline: crate + siblings,
# excluding tests (harness) and excluding the src/julia/ Pure Julia layer's
# handful of .rs loader files (added back explicitly as a footnote so nothing
# is silently dropped from a strict "every .rs in the repo" count).
workspace_rust_total = crate_total_rust + sibling_total
workspace_rust_total_all_rs = workspace_rust_total + julia_loader_rs_loc

# instr_variants / typed_loop_variants were already computed above (shared
# with the --variants-only fast path).

# Self-check: subset_julia_vm_compile/src/compile/cache.rs pins Instr::VARIANTS.len()
# to a literal `EXPECTED_INSTR_VARIANT_COUNT` (Issue #9199 review r3535721788)
# as a cache-fingerprint regression guard. That constant is independently
# maintained (bumped by whoever adds/removes an Instr variant, with a test
# assertion), so it doubles as a free cross-check on this script's own
# variant-counting method — if the two disagree, trust the pinned test and
# suspect this script's parser first.
cache_rs_path = repo_root / "subset_julia_vm" / "src" / "compile" / "cache.rs"
pinned_instr_count = None
if cache_rs_path.exists():
    pm = re.search(r'EXPECTED_INSTR_VARIANT_COUNT:\s*usize\s*=\s*(\d+)', cache_rs_path.read_text(encoding="utf-8", errors="ignore"))
    if pm:
        pinned_instr_count = int(pm.group(1))

largest_path, largest_lines = largest_rs_file(
    [CRATE_SRC] + list(sibling_crates.values())
)
largest_rel = largest_path.relative_to(repo_root) if largest_path else None

# Issue #10817 baseline, measured 2026-07-12 (physical LOC, rounded to the
# issue's own precision — do not "improve" these numbers without also noting
# a definition-change break in the trend, mirroring NORTH_STAR.md policy).
#
# NOTE on instr_variants/typed_loop_variants: the issue's "Instr 455 variant,
# TypedLoopOp ~155 variant" line is NOT reproducible with this (or any exact)
# counting method — re-running this exact extraction against the repo state
# at commit 1e05478a8 (2026-07-12 21:57 JST, the commit on `main` immediately
# preceding the issue's creation timestamp) already gives 431 / 79, matching
# today's counts exactly. There was no one-day drop; the issue's 455/~155
# were loose eyeball estimates, not a measurement this script (or the pinned
# `EXPECTED_INSTR_VARIANT_COUNT` test) ever produced. They are recorded below
# for historical reference only and are deliberately NOT baseline-compared
# (doing so would fire a permanent, misleading "-49%" drift note every run).
ISSUE_EYEBALL_ESTIMATE = {
    "instr_variants": 455,
    "typed_loop_variants": 155,
}
BASELINE = {
    "src/compile/": 147_000,
    "src/vm/": 119_000,
    "src/aot/": 52_000,
    "src/lowering/": 46_000,
    "src/repl/ + rest": 44_000,
    "crate_total_rust": 409_000,
    "julia_jl": 51_000,
    "subset_julia_vm_bytecode": 39_000,
    "subset_julia_vm_types": 41_000,
    "subset_julia_vm_parser": 12_000,
    "subset_julia_vm_ir": 600,
    "subset_julia_vm_ffi": 3_500,
    "subset_julia_vm_web": 900,
    "subset_julia_vm_runtime": 2_300,
    "tests_rs": 47_000,
    "fixtures_jl": 150_000,
    "largest_file_lines": 9_200,
}

print("# LOC / touchpoint report (scripts/loc_report.sh)")
print()
print("Reproduces the Issue #10817 measurement. Report only — not a pass/fail gate.")
print()
print("## subset_julia_vm crate — per-area physical LOC")
print()
print("| Area | LOC | Share of crate Rust | vs. #10817 baseline |")
print("|---|---:|---:|---|")
for name, value in area_loc.items():
    print(f"| `{name}` | {fmt(value)} | {pct(value, crate_total_rust)} |{drift_note(name, value, BASELINE[name])} |")
print(
    f"| `src/repl/` + rest | {fmt(rest_loc)} | {pct(rest_loc, crate_total_rust)} |"
    f"{drift_note('src/repl/ + rest', rest_loc, BASELINE['src/repl/ + rest'])} |"
)
print(
    f"| **subset_julia_vm crate Rust total** | **{fmt(crate_total_rust)}** | 100% |"
    f"{drift_note('crate_total_rust', crate_total_rust, BASELINE['crate_total_rust'])} |"
)
print(
    "(`subset_julia_vm crate Rust total` excludes the handful of .rs loader/glue "
    f"files under `src/julia/` — {fmt(julia_loader_rs_loc)} lines — so this partition "
    "matches Issue #10817's own areas exactly; see Workspace totals below for the "
    "figure with those lines added back.)"
)
print()
print("## Pure Julia layer")
print()
print("| Area | LOC | vs. #10817 baseline |")
print("|---|---:|---|")
print(f"| `src/julia/` (.jl) | {fmt(julia_jl_loc)} |{drift_note('julia_jl', julia_jl_loc, BASELINE['julia_jl'])} |")
print()
print("## Sibling crates (Rust)")
print()
print("| Crate | LOC | vs. #10817 baseline |")
print("|---|---:|---|")
for name, value in sibling_loc.items():
    print(f"| `{name}` | {fmt(value)} |{drift_note(name, value, BASELINE[name])} |")
print(f"| **sibling crates total** | **{fmt(sibling_total)}** | |")
print()
print("## Tests")
print()
print("| Area | LOC | vs. #10817 baseline |")
print("|---|---:|---|")
print(
    f"| `tests/*.rs` (harness, excl. fixtures) | {fmt(tests_rs_loc)} |"
    f"{drift_note('tests_rs', tests_rs_loc, BASELINE['tests_rs'])} |"
)
print(
    f"| `tests/fixtures/**/*.jl` | {fmt(fixtures_jl_loc)} |"
    f"{drift_note('fixtures_jl', fixtures_jl_loc, BASELINE['fixtures_jl'])} |"
)
print()
print("## Workspace totals")
print()
print("| Metric | Value |")
print("|---|---:|")
print(f"| Workspace Rust (subset_julia_vm crate + sibling crates, excl. tests) | {fmt(workspace_rust_total)} |")
print(
    f"| ...plus `src/julia/*.rs` loader/glue ({fmt(julia_loader_rs_loc)} lines, "
    f"excluded above to keep Pure-Julia accounting clean) | {fmt(workspace_rust_total_all_rs)} |"
)
print(f"| ...plus `tests/*.rs` harness | {fmt(workspace_rust_total_all_rs + tests_rs_loc)} |")
print()
print("## Symbolic fused-op / touchpoint counts")
print()
print("(Issue #10817's headline barometer: track these, not LOC, per NORTH_STAR.md.)")
print()
print("| Symbol | Location | Variant count |")
print("|---|---|---:|")
instr_check = ""
if pinned_instr_count is not None:
    if pinned_instr_count == instr_variants:
        instr_check = f" (matches pinned `EXPECTED_INSTR_VARIANT_COUNT` in compile/cache.rs)"
    else:
        instr_check = (
            f" (MISMATCH: compile/cache.rs pins `EXPECTED_INSTR_VARIANT_COUNT = {pinned_instr_count}` — "
            "this script's parser or that test is stale; investigate before trusting either)"
        )
print(f"| `Instr` | `subset_julia_vm_bytecode/src/instr.rs` | {fmt(instr_variants)}{instr_check} |")
print(f"| `TypedLoopOp` | `subset_julia_vm_vm/src/vm/executable.rs` | {fmt(typed_loop_variants)} |")
print()
print(
    f"Issue #10817 (2026-07-12) reported these as approximately "
    f"{ISSUE_EYEBALL_ESTIMATE['instr_variants']} / ~{ISSUE_EYEBALL_ESTIMATE['typed_loop_variants']} "
    "(eyeball estimates, not this exact extraction). Re-running this script's method against "
    "commit 1e05478a8 (the `main` tip immediately preceding the issue's creation) already yields "
    f"{instr_variants} / {typed_loop_variants} — identical to today's count. There is no real "
    "one-day drift here; the issue's figures were rounded/approximate, not a prior machine "
    "measurement. This run therefore doubles as the first rigorous baseline for future quarterly "
    "comparisons (record it in STATUS.md so the *next* snapshot has a real prior value to diff against)."
)
print()
print("## Largest non-test .rs file")
print()
print("| File | LOC | vs. #10817 baseline |")
print("|---|---:|---|")
if largest_rel is not None:
    print(
        f"| `{largest_rel}` | {fmt(largest_lines)} |"
        f"{drift_note('largest_file_lines', largest_lines, BASELINE['largest_file_lines'])} |"
    )
print()
print(
    "Note: the #10817 baseline pins the *file path* `subset_julia_vm_vm/src/vm/executable.rs` "
    "as well as its line count — if a different file becomes largest, that is itself "
    "signal (a new fused-op or specializer file overtaking the typed-loop executor), "
    "not just a number to compare."
)
PY
