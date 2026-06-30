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
#
# Exit code:
#   0 — no category increased and no forbidden stale TODO/placeholder returned
#   1 — one or more structural debt counts increased

set -euo pipefail

if [[ ! -d subset_julia_vm/src ]]; then
    echo "ERROR: subset_julia_vm/src not found. Run from the repository root."
    exit 1
fi

python3 - <<'PY'
from pathlib import Path
import re
import sys

BASELINES = {
    # Issue #8327: Rust string branches on Julia package / non-primitive type names.
    "julia_name_string_branches": 210,
    # Issues #8329/#8335: dispersed environment, target, path, and artifact strings.
    "hardcoded_env_var_reads": 13,
    "hardcoded_target_triples": 14,
    "hardcoded_artifact_names": 32,
    # Issue #8332: panic-prone extraction remains debt; new sites must not appear.
    "unwrap_expect_calls": 1836,
    # Issue #8333: crate-boundary bypasses must not expand.
    "cross_crate_path_attrs": 1,
    # Issue #8334: safe extern C raw-pointer API surface must not expand.
    "ffi_safe_raw_pointer_exports": 22,
    # Issue #8336: large-file and inline-test debt must not grow.
    "large_rs_files_over_2000_lines": 49,
    "large_rust_functions_over_300_lines": 124,
    "src_files_with_inline_tests": 256,
    # Issue #8460: large source-side test blocks should not grow. Public/API
    # tests belong in subset_julia_vm/tests/; keep only private-internal unit
    # tests in src/.
    "large_src_test_blocks_over_200_lines": 84,
    "large_src_test_block_lines_over_200": 52468,
    # Issue #8337: Julia workaround comments still need a full cleanup pass.
    "julia_workaround_comments_without_issue_link": 44,
}

RS_ROOTS = [
    Path("subset_julia_vm/src"),
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
    roots = [Path("subset_julia_vm/src"), Path("subset_julia_vm/packages")]
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
src_rs_files = [path for path in files if str(path).startswith("subset_julia_vm/src")]
large_src_test_block_lengths = [
    length
    for path in src_rs_files
    for length in cfg_test_block_lengths(path)
    if length > 200
]
counts = {
    "julia_name_string_branches": count_regex(files, JULIA_NAME_BRANCH_RE),
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

print("OK: structural debt inventory did not increase (Issues #8327, #8329, #8332, #8333, #8334, #8335, #8336, #8337, #8460).")
for name in sorted(counts):
    print(f"  {name}: {counts[name]} / baseline {BASELINES[name]}")
PY
