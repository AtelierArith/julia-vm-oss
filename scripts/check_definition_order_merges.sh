#!/usr/bin/env bash
# Definition-order merge inventory and bypass gate (Issue #11036).

set -euo pipefail

cd "$(dirname "$0")/.."

python3 - <<'PY'
import csv
import re
import sys
from collections import Counter
from pathlib import Path

inventory_path = Path("docs/vm/DEFINITION_ORDER_MERGE_INVENTORY.tsv")
roots = [
    Path("subset_julia_vm/src"),
    Path("subset_julia_vm_compile/src"),
    Path("subset_julia_vm_lowering/src"),
    Path("subset_julia_vm_vm/src"),
    Path("subset_julia_vm_types/src"),
]
if not inventory_path.is_file():
    print("FAIL: definition-order merge inventory is missing", file=sys.stderr)
    sys.exit(1)

with inventory_path.open(encoding="utf-8", newline="") as handle:
    rows = list(csv.DictReader(handle, delimiter="\t"))

required_columns = {
    "kind", "path", "symbol", "count", "classification", "mechanism", "notes",
    "success_policy", "error_policy", "replay_policy",
}
if not rows or set(rows[0]) != required_columns:
    print("FAIL: definition-order merge inventory has the wrong columns", file=sys.stderr)
    sys.exit(1)

expected = Counter()
failures = []
state_rows = {}
required_state_contracts = {
    ("subset_julia_vm/src/repl/session.rs", "methods:definition_activations"),
    ("subset_julia_vm_vm/src/vm/mod.rs", "methods:repl_definition_activations"),
    (
        "subset_julia_vm_vm/src/vm/mod.rs",
        "runtime_nominals:runtime_nominal_activations",
    ),
    ("subset_julia_vm_vm/src/vm/mod.rs", "imports:repl_using_activations"),
    ("subset_julia_vm_vm/src/vm/mod.rs", "modules:repl_module_activations"),
    ("subset_julia_vm/src/repl/session.rs", "imports:usings"),
    ("subset_julia_vm/src/repl/session.rs", "modules:RecoveredModuleReplay"),
}
allowed_success_policies = {
    "store_reached", "vm_observed", "store_distinct", "store_completed",
}
allowed_error_policies = {
    "typed_prefix", "exact_sites", "reached_statements", "inert_shell",
}
allowed_replay_policies = {
    "merge_definitions", "rebuild_inert", "splice_chronology", "replay_inert_shell",
}
for row in rows:
    if row["kind"] not in {"cursor", "raw", "reviewed", "state"}:
        failures.append("invalid inventory kind {!r}".format(row["kind"]))
        continue
    if not row["classification"] or not row["mechanism"] or not row["notes"]:
        failures.append("incomplete inventory row for {}:{}".format(row["path"], row["symbol"]))
    try:
        count = int(row["count"])
    except ValueError:
        failures.append("non-integer count for {}:{}".format(row["path"], row["symbol"]))
        continue
    if row["kind"] == "state":
        key = (row["path"], row["symbol"])
        if key in state_rows:
            failures.append("duplicate runtime-state inventory row '{}:{}'".format(*key))
        state_rows[key] = count
        if row["classification"] != "persistent_runtime_state":
            failures.append("runtime-state row '{}:{}' has invalid classification".format(*key))
        policies = (
            ("success", row["success_policy"], allowed_success_policies),
            ("error", row["error_policy"], allowed_error_policies),
            ("replay", row["replay_policy"], allowed_replay_policies),
        )
        for phase, policy, allowed in policies:
            if policy not in allowed:
                failures.append(
                    "runtime-state row '{}:{}' has invalid {} policy {!r}".format(
                        key[0], key[1], phase, policy
                    )
                )
    else:
        for column in ("success_policy", "error_policy", "replay_policy"):
            if row[column] != "not_runtime_state":
                failures.append(
                    "non-state row '{}:{}' must use not_runtime_state for {}".format(
                        row["path"], row["symbol"], column
                    )
                )
    if row["kind"] in {"cursor", "raw"}:
        expected[(row["kind"], row["path"], row["symbol"])] += count

for path, symbol in sorted(required_state_contracts - set(state_rows)):
    failures.append("missing runtime-state inventory row '{}'".format(symbol))
for path, symbol in sorted(set(state_rows) - required_state_contracts):
    failures.append("unrecognized runtime-state inventory row '{}:{}'".format(path, symbol))

for (path, symbol), expected_count in sorted(state_rows.items()):
    source_path = Path(path)
    if not source_path.is_file():
        failures.append("runtime-state source is missing: {}".format(path))
        continue
    family, separator, evidence = symbol.partition(":")
    if not family or not separator or not evidence:
        failures.append("runtime-state symbol must use family:evidence: {!r}".format(symbol))
        continue
    source = source_path.read_text(encoding="utf-8", errors="ignore")
    if evidence == "RecoveredModuleReplay":
        declaration_re = re.compile(r"^struct\s+RecoveredModuleReplay\s*\{", re.MULTILINE)
    else:
        declaration_re = re.compile(
            r"^\s*(?:pub\s+)?" + re.escape(evidence) + r"\s*:\s*Vec<",
            re.MULTILINE,
        )
    actual_count = len(declaration_re.findall(source))
    if actual_count != expected_count:
        failures.append(
            "runtime-state evidence {}:{} expected {} declaration(s), found {}".format(
                path, evidence, expected_count, actual_count
            )
        )

activation_paths = [
    Path("subset_julia_vm/src/repl/session.rs"),
    Path("subset_julia_vm_vm/src/vm/mod.rs"),
]
activation_re = re.compile(
    r"^\s*(?:pub\s+)?([A-Za-z_][A-Za-z0-9_]*_activations)\s*:\s*Vec<",
    re.MULTILINE,
)
discovered_activations = {
    (path.as_posix(), evidence)
    for path in activation_paths
    for evidence in activation_re.findall(path.read_text(encoding="utf-8", errors="ignore"))
}
inventoried_activations = {
    (path, symbol.partition(":")[2])
    for path, symbol in state_rows
    if symbol.partition(":")[2].endswith("_activations")
}
for path, evidence in sorted(discovered_activations - inventoried_activations):
    failures.append(
        "activation collection {}:{} lacks a runtime-state inventory row".format(path, evidence)
    )
for path, evidence in sorted(inventoried_activations - discovered_activations):
    failures.append(
        "stale activation inventory evidence {}:{}".format(path, evidence)
    )

fn_re = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)"
)
cursor_re = re.compile(
    r"\b[A-Za-z_][A-Za-z0-9_]*\s*\.\s*"
    r"(?:append_fragment|insert_fragment_after)\s*\("
)
raw_re = re.compile(
    r"\b[A-Za-z_][A-Za-z0-9_]*\s*\.\s*"
    r"(?:functions|structs|abstract_types|primitive_types|modules|submodules)\s*\.\s*"
    r"(?:push|extend|append|insert|splice)\s*\("
)
raw_assignment_re = re.compile(
    r"\b[A-Za-z_][A-Za-z0-9_]*\s*\.\s*"
    r"(?:functions|structs|abstract_types|primitive_types|modules|submodules)\s*=(?!=)"
)
alias_re = re.compile(
    r"\blet\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*&mut\s+"
    r"[A-Za-z_][A-Za-z0-9_]*\s*\.\s*"
    r"(?:functions|structs|abstract_types|primitive_types|modules|submodules)\b"
)
alias_mutation_template = r"\b(?:{})\s*\.\s*(?:push|extend|append|insert|splice)\s*\("
forbidden_re = re.compile(r"\b(?:shift_definition_orders|max_definition_order|push_module_in_definition_order)\b")

actual = Counter()
for root in roots:
    for path in sorted(root.rglob("*.rs")):
        rel = path.as_posix()
        if "/tests/" in rel or rel.endswith("/tests.rs"):
            continue
        symbol = "<module>"
        pending_test = False
        function_is_test = False
        aliases = set()
        brace_depth = 0
        function_stack = []
        for line_no, line in enumerate(path.read_text(encoding="utf-8", errors="ignore").splitlines(), 1):
            stripped = line.strip()
            if stripped == "#[test]" or stripped.startswith("#[tokio::test"):
                pending_test = True
                continue
            match = fn_re.match(line)
            if match:
                function_stack.append({
                    "depth": brace_depth,
                    "opened": "{" in line,
                    "symbol": symbol,
                    "is_test": function_is_test,
                    "aliases": aliases,
                })
                symbol = match.group(1)
                function_is_test = pending_test
                pending_test = False
                aliases = set()
            elif stripped and not stripped.startswith("#"):
                pending_test = False

            if forbidden_re.search(line) and rel != "subset_julia_vm_types/src/ir/core.rs":
                failures.append(
                    "direct definition-order offset helper outside IR owner at {}:{}".format(rel, line_no)
                )
            if function_is_test:
                continue
            if cursor_re.search(line):
                actual[("cursor", rel, symbol)] += 1
            alias_match = alias_re.search(line)
            if alias_match:
                aliases.add(alias_match.group(1))
            alias_mutation_re = re.compile(
                alias_mutation_template.format("|".join(re.escape(alias) for alias in aliases))
            ) if aliases else None
            if (raw_re.search(line) or raw_assignment_re.search(line)
                    or (alias_mutation_re and alias_mutation_re.search(line))):
                actual[("raw", rel, symbol)] += 1

            brace_depth += line.count("{") - line.count("}")
            if function_stack and not function_stack[-1]["opened"] and "{" in line:
                function_stack[-1]["opened"] = True
            while (function_stack and function_stack[-1]["opened"]
                    and brace_depth <= function_stack[-1]["depth"]):
                previous = function_stack.pop()
                symbol = previous["symbol"]
                function_is_test = previous["is_test"]
                aliases = previous["aliases"]

for key in sorted(set(expected) | set(actual)):
    if expected[key] != actual[key]:
        kind, path, symbol = key
        failures.append(
            "{} site {}:{} expected {} occurrence(s), found {}".format(
                kind, path, symbol, expected[key], actual[key]
            )
        )

if failures:
    print("FAIL: definition-order merge inventory drift (Issue #11036):", file=sys.stderr)
    for failure in failures:
        print("  - " + failure, file=sys.stderr)
    print(
        "Route independent lowered fragments through DefinitionOrderCursor chronology APIs "
        "and classify the boundary in {}.".format(inventory_path),
        file=sys.stderr,
    )
    sys.exit(1)

print(
    "OK: definition-order merge inventory covers {} cursor and {} raw Core-IR mutation sites "
    "plus {} runtime-state contracts across {} classified boundaries "
    "(Issues #11036/#11740).".format(
        sum(value for (kind, _, _), value in actual.items() if kind == "cursor"),
        sum(value for (kind, _, _), value in actual.items() if kind == "raw"),
        len(state_rows),
        len(rows),
    )
)
PY
