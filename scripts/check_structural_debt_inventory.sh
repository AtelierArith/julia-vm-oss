#!/usr/bin/env bash
# check_structural_debt_inventory.sh
#
# Ratchet broad structural debt categories surfaced in milestone 56.
#
# This is intentionally an inventory gate, not a one-shot rewrite. The current
# codebase still carries legacy hardcoded Julia names, duplicated configuration
# strings, unwrap/expect sites, broad public/FFI surfaces, large files, and
# inline tests. The audit makes that debt visible and prevents new instances
# from landing silently while the larger refactors proceed issue by issue.
#
# Usage:
#   bash scripts/check_structural_debt_inventory.sh
#   bash scripts/check_structural_debt_inventory.sh --update   # rewrite BASELINES
#     to the current counts in this script (bumps AND tightens every row; does
#     NOT clear forbidden stale TODO/placeholder markers, which have no
#     baseline to bump — those must be fixed by hand). Part of the one-command
#     baseline refresh for Issue #10870; run this, review the diff (it should
#     touch only numeric literals in the BASELINES dict), then re-run without
#     --update to confirm green.
#
# Exit code:
#   0 — no category increased and no forbidden stale TODO/placeholder returned
#       (or, with --update, the rewrite succeeded and no stale markers remain)
#   1 — one or more structural debt counts increased

set -euo pipefail

if [[ ! -d subset_julia_vm/src ]]; then
    echo "ERROR: subset_julia_vm/src not found. Run from the repository root."
    exit 1
fi

SJULIA_STRUCTURAL_DEBT_UPDATE=0
if [[ "${1:-}" == "--update" ]]; then
    SJULIA_STRUCTURAL_DEBT_UPDATE=1
fi
SJULIA_STRUCTURAL_DEBT_SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
export SJULIA_STRUCTURAL_DEBT_UPDATE SJULIA_STRUCTURAL_DEBT_SELF

python3 - <<'PY'
from pathlib import Path
import os
import re
import sys

BASELINES = {
    # Issue #8327: Rust string branches on Julia package / non-primitive type names.
    # 187 -> 188 (Issue #8995): exact malformed-Char handling adds one reviewed
    # Julia-type classification branch.
    "julia_name_string_branches": 184,
    # Issue #9920: module and function-name string branches need independent
    # visibility while the broad Julia-name audit is retired incrementally.
    # 75 -> 76 (Issue #11369): owner-qualified Base-parametric shadow routing
    # adds one reviewed module-name branch while preserving the tighter
    # function-name baseline from this branch.
    # 76 -> 88 (Issue #11410): Core/Base binding-authority projection and
    # ordinary/bare-module provenance add reviewed namespace branches.
    "module_name_string_branches": 88,
    "function_name_string_branches": 123,
    # Issues #8329/#8335: dispersed environment, target, path, and artifact strings.
    "hardcoded_env_var_reads": 23,
    "hardcoded_target_triples": 15,
    "hardcoded_artifact_names": 34,
    # Issue #8332: panic-prone extraction remains debt; new sites must not appear.
    # 1801 -> 1812 (Issue #11078): +11, ALL of them `.expect(` inside `#[cfg(test)]`
    # blocks of the new StructId/StructRegistry tests (cache.rs, type_helpers.rs,
    # struct_registry.rs). No production unwrap/expect was added — the compile
    # subtree stays clippy zero-denied (`unwrap_used`/`expect_used`), which is
    # what the panic-free POLICY (docs/vm/PANIC_FREE.md) actually forbids; this
    # ratchet is a text scanner that cannot see the cfg(test) boundary.
    # 1812 -> 1814 (Issue #11281): two `.expect(` calls live in the new
    # bare-local / try-clause lowering regression test; production lowering
    # remains clippy zero-denied.
    # 1814 -> 1865 (Issues #11147/#11372): the constructor-owner/cache and
    # transient-root/splat regression suites add 51 test-only unwrap/expect
    # sites. The one new production panic surfaced by this audit was instead
    # converted to a propagated InternalError.
    "unwrap_expect_calls": 1886,
    # Issue #8333: crate-boundary bypasses must not expand.
    "cross_crate_path_attrs": 1,
    # Issue #8334: safe extern C raw-pointer API surface must not expand.
    "ffi_safe_raw_pointer_exports": 39,
    # Issue #8336: large-file and inline-test debt must not grow.
    # 61 -> 63 (Issues #11179/#11197): the constructor-dispatch work merged
    # through PR #11136 moved one compile source over the threshold. The
    # authority-aware nested-function collector also crossed it while making
    # runtime @eval a hard new-owner scope boundary. These are reviewed feature
    # growth; the count remains a ratchet.
    # 63 -> 65 (Issues #11147/#8995): owner-preserving module constructor
    # routing, generic iteration, and invalid-Char capture classification move
    # two reviewed files across the threshold.
    "large_rs_files_over_2000_lines": 65,
    # 126 -> 127 (Issues #11569/#9784): the hard-scope comprehension compiler
    # now owns explicit lexical entry/exit and all exceptional cleanup paths in
    # one reviewed private routine. Follow-up extraction remains tracked by the
    # large-function ratchet rather than splitting transactional cleanup today.
    "large_rust_functions_over_300_lines": 126,
    # 251 -> 252 (Issue #9090): the extracted compiler crate owns a crate-local
    # test host so its unit-test build initializes the correct crate instance.
    # 252 -> 255 (Issues #11147/#11372): private constructor, iteration, and
    # rooted field-projection unit tests stay beside their crate-private APIs.
    # 256 -> 257 (Issue #10607): private comprehension representation routing
    # has white-box coverage beside its crate-private classifier; public
    # comprehension behavior remains covered by fixtures/integration tests.
    "src_files_with_inline_tests": 257,
    # Issue #8460: large source-side test blocks should not grow. Public/API
    # tests belong in subset_julia_vm/tests/; keep only private-internal unit
    # tests in src/.
    # 95 -> 96 (Issue #11372): the private generic-iteration/rooting test module
    # crosses the threshold while covering Julia-protocol splat validation.
    "large_src_test_blocks_over_200_lines": 98,
    # 57584 -> 57722 (Issue #11078): the new owner-scoped-id tests (shadow-not-
    # destroy, fresh-vs-cache-restore StructId parity, module-registry agreement,
    # and the seeding negative control) live in existing >200-line src test blocks.
    # 57724 -> 57759 (Issue #11179): PR #11136 extended existing large private
    # inference/dispatch test modules while adding runtime type-argument
    # constructor coverage. No new large test block was introduced.
    # 57767 -> 57787 (Issue #10460): structured generic-alias parameter
    # regression coverage extends the existing vm/type_utils.rs test module;
    # no new inline test block was introduced.
    # 57787 -> 57861 (Issue #10460, after merging current main): source-binder
    # rebind, runtime Vararg, and dependent-bound display regressions extend
    # existing private unit-test modules; no new inline test block was introduced.
    # 57790 -> 57845 (Issue #10334): private Base-cache section serializers and
    # compile-context restore scoreboards gain persisted-policy white-box
    # coverage; the public `.sjvmbc` regression lives under integration tests.
    # 58581 -> 58750 (Issue #10460): binder identity, dependent-bound,
    # structured-cache, and runtime type-object regressions require private
    # white-box helpers across the existing source-side test modules. No new
    # large test block or test binary is introduced.
    # 58750 -> 58772 (Issues #11569/#10607): lexical-owner, transactional heap,
    # and private comprehension-routing white-box regressions extend source-side
    # modules; end-to-end hard-scope parity also lives in consolidated tests.
    # 58751 -> 58785 (Issue #11297): synchronize the ratchet with exact
    # origin/main after guarded merges left the source-only audit red. The #9784
    # branch adds no source-side test lines; its white-box test was removed in
    # favor of the consolidated public REPL regression coverage.
    "large_src_test_block_lines_over_200": 58878,
    # Issue #8337: Julia workaround comments still need a full cleanup pass.
    "julia_workaround_comments_without_issue_link": 42,
}

RS_ROOTS = [
    Path("subset_julia_vm/src"),
    Path("subset_julia_vm_lowering/src"),
    Path("subset_julia_vm_compile/src"),
    Path("subset_julia_vm_vm/src"),
    Path("subset_julia_vm_ffi/src"),
    Path("subset_julia_vm_runtime/src"),
    Path("subset_julia_vm_parser/src"),
    Path("subset_julia_vm_web/src"),
]

JULIA_NAME_BRANCH_RE = re.compile(
    r'(?:==|!=|starts_with|ends_with|contains|matches!\s*\(|match\s+)'
    r'.{0,120}"(?:Plots|JSXGraph|Symbolics|Distributions|SciMLBase|'
    r'OrdinaryDiffEq|Array|Vector|Matrix|Dict|Set|Complex|Broadcasted|'
    r'Ref|Memory|MemoryRef|Tuple)"'
)
MODULE_NAME_BRANCH_RE = re.compile(
    r'(?:==|!=|starts_with|ends_with|contains|matches!\s*\(|match\s+)'
    r'.{0,120}"(?:Base|Core|Meta|Random|Iterators|LinearAlgebra|'
    r'Core\.Intrinsics|Core\.Compiler|Base\.MathConstants|Sys|Main|'
    r'Test|MacroTools)"'
)
FUNCTION_NAME_BRANCH_RE = re.compile(
    r'(?:==|!=|starts_with|ends_with|contains|matches!\s*\(|match\s+)'
    r'.{0,120}"(?:Dict|copy|show|print|collect|getindex|Generator|'
    r'promote_rule|IteratorEltype|IteratorSize|convert|Set)"'
)
ENV_READ_RE = re.compile(
    r'\b(?:std::)?env::var(?:_os)?\s*\(\s*"'
    r'(?:SJULIA|SUBSETJULIA|SUBSET_JULIA_VM|JULIA)_[^"]+"'
    r'|\boption_env!\s*\(\s*"'
    r'(?:SJULIA|SUBSETJULIA|SUBSET_JULIA_VM|JULIA)_[^"]+"'
)
TARGET_TRIPLE_RE = re.compile(
    r'"(?:x86_64-unknown-linux-gnu|x86_64-apple-darwin|'
    r'x86_64-pc-windows-msvc|aarch64-apple-ios(?:-sim)?|'
    r'wasm32-unknown-unknown)"'
)
ARTIFACT_NAME_RE = re.compile(
    r'"(?:libsjulia_runtime\.a|libapp\.(?:so|dylib)|app\.dll|'
    r'main\.(?:o|obj)|[^"\\]*(?:\.sjir|\.sjvmbc|\.ji\.json)|'
    r'sjulia_(?:base_cache|prelude_program)[^"\\]*\.bin)"'
)
UNWRAP_EXPECT_RE = re.compile(r'\.(?:unwrap|expect)\s*\(')
CROSS_CRATE_PATH_RE = re.compile(r'#\s*\[\s*path\s*=\s*"\.\.')


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="ignore")


def rust_files() -> list[Path]:
    files: list[Path] = []
    for root in RS_ROOTS:
        if root.exists():
            files.extend(sorted(root.rglob("*.rs")))
    return files


def source_files_for_stale_todos() -> list[Path]:
    roots = [
        Path("subset_julia_vm/src"),
        Path("subset_julia_vm_lowering/src"),
        Path("subset_julia_vm_compile/src"),
        Path("subset_julia_vm_vm/src"),
        Path("subset_julia_vm/packages"),
    ]
    files: list[Path] = []
    for root in roots:
        if root.exists():
            for path in root.rglob("*"):
                if path.is_file() and path.suffix in {".rs", ".jl", ".md"}:
                    files.append(path)
    return files


def count_regex(files: list[Path], pattern: re.Pattern[str]) -> int:
    return sum(len(pattern.findall(read_text(path))) for path in files)


def count_safe_ffi_raw_pointer_exports(files: list[Path]) -> int:
    count = 0
    for path in files:
        if not str(path).startswith("subset_julia_vm_ffi/src"):
            continue
        lines = read_text(path).splitlines()
        i = 0
        while i < len(lines):
            line = lines[i]
            if 'pub extern "C" fn' not in line and 'pub unsafe extern "C" fn' not in line:
                i += 1
                continue
            signature = line
            j = i + 1
            while j < len(lines) and "{" not in signature and ";" not in signature:
                signature += " " + lines[j].strip()
                j += 1
            if 'pub extern "C" fn' in signature and re.search(r'\*(?:const|mut)\b', signature):
                count += 1
            i = max(j, i + 1)
    return count


def count_large_rust_functions(files: list[Path], threshold: int) -> int:
    fn_start = re.compile(
        r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?(?:extern\s+"C"\s+)?'
        r'(?:async\s+)?fn\s+\w'
    )
    large = 0
    for path in files:
        lines = read_text(path).splitlines()
        i = 0
        while i < len(lines):
            if not fn_start.search(lines[i]):
                i += 1
                continue
            start = i
            depth = 0
            seen_open = False
            while i < len(lines):
                depth += lines[i].count("{")
                if "{" in lines[i]:
                    seen_open = True
                depth -= lines[i].count("}")
                if seen_open and depth <= 0:
                    break
                i += 1
            if seen_open and (i - start + 1) > threshold:
                large += 1
            i += 1
    return large


def cfg_test_block_lengths(path: Path) -> list[int]:
    lines = read_text(path).splitlines()
    lengths: list[int] = []
    i = 0
    while i < len(lines):
        if "#[cfg(test)]" not in lines[i]:
            i += 1
            continue

        start = i + 1
        j = i + 1
        while j < len(lines) and (
            lines[j].strip().startswith("#[")
            or lines[j].strip() == ""
            or lines[j].strip().startswith("//")
        ):
            j += 1
        if j >= len(lines):
            break

        lookahead = "\n".join(lines[j : min(len(lines), j + 3)])
        if "{" not in lookahead:
            end = j + 1
        else:
            depth = 0
            seen_open = False
            end = j
            for k in range(j, len(lines)):
                for ch in lines[k]:
                    if ch == "{":
                        depth += 1
                        seen_open = True
                    elif ch == "}":
                        depth -= 1
                if seen_open and depth == 0:
                    end = k + 1
                    break
        lengths.append(end - start + 1)
        i = end
    return lengths


def count_julia_workarounds_without_issue() -> int:
    roots = [Path("subset_julia_vm/src"), Path("subset_julia_vm/packages")]
    count = 0
    for root in roots:
        if not root.exists():
            continue
        for path in root.rglob("*.jl"):
            for line in read_text(path).splitlines():
                if "# Workaround:" in line and not re.search(r'Issue #[0-9]+', line):
                    count += 1
    return count


files = rust_files()
core_src_prefixes = (
    "subset_julia_vm/src",
    "subset_julia_vm_lowering/src",
    "subset_julia_vm_compile/src",
    "subset_julia_vm_vm/src",
)
src_rs_files = [path for path in files if str(path).startswith(core_src_prefixes)]
large_src_test_block_lengths = [
    length
    for path in src_rs_files
    for length in cfg_test_block_lengths(path)
    if length > 200
]
counts = {
    "julia_name_string_branches": count_regex(files, JULIA_NAME_BRANCH_RE),
    "module_name_string_branches": count_regex(files, MODULE_NAME_BRANCH_RE),
    "function_name_string_branches": count_regex(files, FUNCTION_NAME_BRANCH_RE),
    "hardcoded_env_var_reads": count_regex(files, ENV_READ_RE),
    "hardcoded_target_triples": count_regex(files, TARGET_TRIPLE_RE),
    "hardcoded_artifact_names": count_regex(files, ARTIFACT_NAME_RE),
    "unwrap_expect_calls": count_regex(files, UNWRAP_EXPECT_RE),
    "cross_crate_path_attrs": count_regex(files, CROSS_CRATE_PATH_RE),
    "ffi_safe_raw_pointer_exports": count_safe_ffi_raw_pointer_exports(files),
    "large_rs_files_over_2000_lines": sum(
        1
        for path in src_rs_files
        if len(read_text(path).splitlines()) > 2000
    ),
    "large_rust_functions_over_300_lines": count_large_rust_functions(src_rs_files, 300),
    "src_files_with_inline_tests": sum(
        1
        for path in src_rs_files
        if "#[test]" in read_text(path) or "#[cfg(test)]" in read_text(path)
    ),
    "large_src_test_blocks_over_200_lines": len(large_src_test_block_lengths),
    "large_src_test_block_lines_over_200": sum(large_src_test_block_lengths),
    "julia_workaround_comments_without_issue_link": count_julia_workarounds_without_issue(),
}

violations: list[str] = []
for name, actual in sorted(counts.items()):
    allowed = BASELINES[name]
    if actual > allowed:
        violations.append(f"{name}: {actual} > baseline {allowed}")

stale_todos: list[str] = []
placeholder_hits: list[str] = []
for path in source_files_for_stale_todos():
    for lineno, line in enumerate(read_text(path).splitlines(), start=1):
        if "TODO" in line and re.search(r'#(?:1447|3510)\b', line):
            stale_todos.append(f"{path}:{lineno}: {line.strip()}")
        if "Issue #XXXX" in line:
            placeholder_hits.append(f"{path}:{lineno}: {line.strip()}")

if stale_todos:
    violations.append(
        "closed-issue TODO references returned:\n  " + "\n  ".join(stale_todos)
    )
if placeholder_hits:
    violations.append(
        "Issue #XXXX placeholders returned in active source:\n  "
        + "\n  ".join(placeholder_hits)
    )

if os.environ.get("SJULIA_STRUCTURAL_DEBT_UPDATE") == "1":
    # --update (Issue #10870): rewrite this script's own BASELINES dict to the
    # current counts — bumping over-baseline rows AND tightening
    # under-baseline rows in one mechanical pass. Never touches the stale
    # closed-issue TODO / Issue #XXXX placeholder checks below, which are
    # hard-forbidden (no baseline to bump); those must be fixed by hand.
    self_path = Path(os.environ["SJULIA_STRUCTURAL_DEBT_SELF"])
    text = self_path.read_text(encoding="utf-8")
    changes: list[tuple[str, int, int]] = []
    for name, actual in sorted(counts.items()):
        old = BASELINES[name]
        if old == actual:
            continue
        pattern = re.compile(r'("' + re.escape(name) + r'":\s*)(\d+)(,)')
        new_text, n = pattern.subn(
            lambda m, actual=actual: m.group(1) + str(actual) + m.group(3),
            text,
            count=1,
        )
        if n != 1:
            print(
                f"ERROR: --update could not find a unique BASELINES line for "
                f"{name!r} in {self_path} (found {n} matches)",
                file=sys.stderr,
            )
            sys.exit(2)
        text = new_text
        changes.append((name, old, actual))
    self_path.write_text(text, encoding="utf-8")

    if changes:
        print(f"Updated {len(changes)} baseline(s) in {self_path}:")
        for name, old, actual in changes:
            direction = "bumped" if actual > old else "tightened"
            print(f"  {name}: {old} -> {actual} ({direction})")
    else:
        print("No baseline changes needed; every count already matches its baseline.")

    if stale_todos or placeholder_hits:
        print("")
        print(
            "ERROR: --update does not clear forbidden stale markers (no baseline "
            "to bump) — fix these by hand, then re-run without --update:"
        )
        for v in stale_todos + placeholder_hits:
            print(f"  {v}")
        sys.exit(1)

    print("Re-run without --update to confirm a green check.")
    sys.exit(0)

if violations:
    print("ERROR: structural debt inventory increased or stale markers returned.")
    print("")
    for violation in violations:
        print(f"- {violation}")
    print("")
    print("Current inventory:")
    for name in sorted(counts):
        print(f"  {name}: {counts[name]} (baseline {BASELINES[name]})")
    sys.exit(1)

print("OK: structural debt inventory did not increase (Issues #8327, #8329, #8332, #8333, #8334, #8335, #8336, #8337, #8460, #9920).")
for name in sorted(counts):
    print(f"  {name}: {counts[name]} / baseline {BASELINES[name]}")
PY
