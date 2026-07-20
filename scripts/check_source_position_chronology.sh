#!/usr/bin/env bash
# Enforce source-identity-bearing chronology comparisons in lowering (Issue #11100).
set -euo pipefail

cd "$(dirname "$0")/.."

python3 - <<'PY'
import pathlib
import re
import sys


root = pathlib.Path("subset_julia_vm_lowering/src")
authority_path = root / "lowering/type_alias.rs"
if not authority_path.is_file():
    print("ERROR: source-position authority is missing: {} (Issue #11100)".format(authority_path), file=sys.stderr)
    sys.exit(1)

authority = authority_path.read_text(encoding="utf-8")
failures = []


def function_params(name):
    match = re.search(
        r"(?ms)^pub fn " + re.escape(name) + r"\s*\((.*?)\)\s*(?:->\s*[^\{]+)?\{",
        authority,
    )
    if match is None:
        failures.append("missing public chronology API '{}'".format(name))
        return ""
    return re.sub(r"\s+", " ", match.group(1)).strip()


required_params = {
    "register_prescanned": "definition_position: SourcePosition",
    "register_prescanned_non_alias": "definition_position: SourcePosition",
    "expand_for_signature": "use_position: SourcePosition",
}
for function_name, required in required_params.items():
    params = function_params(function_name)
    if required not in params:
        failures.append(
            "{} must accept '{}' instead of a raw offset".format(function_name, required)
        )
    if re.search(r"\b(?:definition|use)_(?:start|offset)\s*:\s*usize\b", params):
        failures.append("{} accepts a raw source-order usize".format(function_name))

normalized = re.sub(r"\s+", " ", authority)
required_helper = (
    "fn definition_is_visible_at(self, use_position: Self) -> bool { "
    "self.source != use_position.source || self.byte_offset <= use_position.byte_offset }"
)
if required_helper not in normalized:
    failures.append(
        "SourcePosition::definition_is_visible_at must compare offsets only after carrying both source identities"
    )

if "pub fn current_source_position(byte_offset: usize) -> Option<SourcePosition>" not in normalized:
    failures.append(
        "current_source_position must construct an opaque position only from an active SourceScope"
    )

if "_not_send: PhantomData<Rc<()>>" not in normalized:
    failures.append("SourceScope must remain non-Send because it restores thread-local state")

required_active_guard = (
    "let active = CURRENT_SOURCE.with(Cell::get); assert_eq!( active, Some(self.identity), "
    '"SourceScope::position requires this scope to be active" );'
)
if required_active_guard not in normalized:
    failures.append("SourceScope::position must reject a retained inactive scope")

raw_param = re.compile(
    r"\b(?:definition|use|alias|binding)_(?:start|offset)\s*:\s*usize\b"
)
for match in re.finditer(r"(?ms)^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)\s*\((.*?)\)\s*(?:->\s*[^\{]+)?\{", "\n".join(
    path.read_text(encoding="utf-8") for path in sorted(root.rglob("*.rs"))
)):
    if raw_param.search(match.group(2)):
        failures.append("lowering chronology API '{}' accepts a raw source-order usize".format(match.group(1)))

comparison = re.compile(r"\s(?:<=|>=|<|>)\s|\.cmp\s*\(")
semantic_offset = re.compile(r"\b(?:definition|use|alias|binding)_(?:start|offset)\b")
raw_comparisons = []
byte_offset_comparisons = []
for path in sorted(root.rglob("*.rs")):
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        code = raw_line.split("//", 1)[0]
        if not comparison.search(code):
            continue
        relative = path.as_posix()
        if semantic_offset.search(code) or re.search(r"\b(?:\w+\.)*span\.start\b", code):
            raw_comparisons.append("{}:{}: {}".format(relative, line_number, code.strip()))
        if "byte_offset" in code:
            byte_offset_comparisons.append("{}:{}: {}".format(relative, line_number, code.strip()))

if raw_comparisons:
    failures.append(
        "raw source-order offset comparison(s) bypass SourcePosition:\n  "
        + "\n  ".join(raw_comparisons)
    )

if len(byte_offset_comparisons) != 1 or "self.byte_offset <= use_position.byte_offset" not in byte_offset_comparisons[0]:
    failures.append(
        "byte_offset chronology must have exactly one reviewed comparison inside SourcePosition; found:\n  "
        + ("\n  ".join(byte_offset_comparisons) if byte_offset_comparisons else "<none>")
    )

if authority.count("byte_offset") != 7:
    failures.append(
        "SourcePosition byte_offset access escaped its field, two constructors, or reviewed comparison"
    )

if failures:
    for failure in failures:
        print("ERROR: {} (Issue #11100)".format(failure), file=sys.stderr)
    sys.exit(1)

print("OK: lowering source chronology is carried by SourcePosition (Issue #11100)")
PY
