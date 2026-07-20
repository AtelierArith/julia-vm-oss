#!/usr/bin/env bash
# Keep typed exception side channels behind the keyed one-shot carrier (#11647).

set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
import pathlib
import re
import sys

root = pathlib.Path("subset_julia_vm_vm/src/vm")
owner = root / "exec/exception_payload.rs"
errors = []


def function_section(text, name):
    pattern = re.compile(
        r"(?m)^ {4}(?:pub(?:\([^)]*\))?\s+)?fn " + re.escape(name) + r"\s*\("
    )
    match = pattern.search(text)
    if match is None:
        errors.append(f"missing reviewed function '{name}'")
        return ""
    next_function = re.compile(
        r"(?m)^ {4}(?:pub(?:\([^)]*\))?\s+)?fn [A-Za-z_][A-Za-z0-9_]*\s*\("
    ).search(text, match.end())
    end = next_function.start() if next_function is not None else len(text)
    return text[match.start():end]


def vm_fields(text):
    declaration = re.search(r"(?m)^pub struct Vm(?:<[^\n{]+>)?\s*\{", text)
    if declaration is None:
        errors.append("missing Vm struct declaration")
        return {}
    following_impl = re.search(r"(?m)^impl(?:<[^\n{]+>)?\s+Vm(?:<[^\n{]+>)?\s*\{", text[declaration.end():])
    if following_impl is None:
        errors.append("missing Vm implementation after struct declaration")
        return {}
    end = declaration.end() + following_impl.start()
    body = re.sub(r"(?m)//.*$", "", text[declaration.start():end])
    body = re.sub(r"(?m)^\s*#\[[^\n]*\]\s*$", "", body)
    fields = {}
    chunk = []
    angle = paren = bracket = brace = 0
    for char in body[body.index("{") + 1:]:
        if char == "<":
            angle += 1
        elif char == ">" and angle:
            angle -= 1
        elif char == "(":
            paren += 1
        elif char == ")" and paren:
            paren -= 1
        elif char == "[":
            bracket += 1
        elif char == "]" and bracket:
            bracket -= 1
        elif char == "{":
            brace += 1
        elif char == "}" and brace:
            brace -= 1
        if char == "," and not (angle or paren or bracket or brace):
            declaration = "".join(chunk).strip()
            chunk = []
            match = re.match(r"^([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(.+)$", declaration, re.S)
            if match is not None:
                fields[match.group(1)] = re.sub(r"\s+", " ", match.group(2)).strip()
        else:
            chunk.append(char)
    return fields


def without_comments(text):
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"(?m)//.*$", "", text)

legacy_names = re.compile(
    r"\bpending_(?:method_error_payload|domain_error_val|type_error_payload|"
    r"string_index_error_payload|parse_error_payload|field_index_error_receiver)\b"
)
ad_hoc_name = re.compile(
    r"\bpending_[A-Za-z0-9_]*(?:error|exception)[A-Za-z0-9_]*payload[A-Za-z0-9_]*\b"
)

for path in sorted(root.rglob("*.rs")):
    text = path.read_text()
    semantic_text = without_comments(text)
    for match in legacy_names.finditer(semantic_text):
        line = semantic_text.count("\n", 0, match.start()) + 1
        errors.append(f"{path}:{line}: legacy exception payload carrier '{match.group()}'")
    for match in ad_hoc_name.finditer(semantic_text):
        if match.group() == "pending_exception_payload":
            continue
        line = semantic_text.count("\n", 0, match.start()) + 1
        errors.append(f"{path}:{line}: ad-hoc exception payload carrier '{match.group()}'")

mod_text = (root / "mod.rs").read_text()
state_text = (root / "state.rs").read_text()
handling_text = (root / "exec/error_handling.rs").read_text()
owner_text = owner.read_text()

fields = vm_fields(mod_text)
canonical_type = fields.get("pending_exception_payload")
if canonical_type != "exec::exception_payload::PendingExceptionPayloadCarrier":
    errors.append("Vm must own exactly one canonical pending_exception_payload field")

reviewed_value_fields = {
    "stack",
    "arg_vec_pool",
    "weak_refs",
    "pending_finalizers",
    "pending_error",
    "pending_exception_value",
    "caught_exceptions",
    "pending_finally_rethrows",
    "generated_expr_cache",
    "pending_exception_payload",
}
for name, field_type in fields.items():
    if re.search(r"\b(?:Value|VmError)\b|Payload|Exception", field_type) and name not in reviewed_value_fields:
        errors.append(
            f"unreviewed Vm field can carry exception values: {name}: {field_type}"
        )

constructor_initializers = re.findall(
    r"pending_exception_payload\s*:\s*(?:Default|PendingExceptionPayloadCarrier)::default\s*\(\s*\)",
    state_text,
)

funnel = function_section(handling_text, "vm_error_to_exception_value")
park_adapter = function_section(handling_text, "exception_error_with_payload")
attach_adapter = function_section(handling_text, "attach_exception_payload")
clear_adapter = function_section(handling_text, "clear_pending_exception_payloads")
owner_park = function_section(owner_text, "park_and_construct")
owner_attach = function_section(owner_text, "park_for_existing")
owner_take = function_section(owner_text, "take_fields_for")
owner_clear = function_section(owner_text, "clear")

required_counts = [
    (len(constructor_initializers), 2,
     "both Vm constructors must initialize the canonical carrier"),
    (park_adapter.count(".park_and_construct("), 1,
     "the funnel must have one atomic park-and-construct boundary"),
    (attach_adapter.count(".park_for_existing("), 1,
     "the funnel must have one checked existing-error attachment boundary"),
    (funnel.count(".take_fields_for("), 1,
     "the funnel must unconditionally consume through the canonical boundary"),
    (clear_adapter.count(".clear("), 1,
     "recovery must clear through the canonical boundary"),
    (owner_park.count("self.pending = Some("), 1,
     "the carrier owner park method must own exactly one payload write"),
    (owner_attach.count("self.pending = Some("), 1,
     "the carrier owner attach method must own exactly one payload write"),
    (owner_take.count(".pending.take("), 1,
     "the carrier owner take method must own exactly one payload consume"),
    (owner_clear.count("self.pending = None"), 1,
     "the carrier owner clear method must own exactly one payload clear"),
]
for actual, expected, message in required_counts:
    if actual != expected:
        errors.append(f"{message}: expected {expected}, found {actual}")

take_position = funnel.find(".take_fields_for(")
classify_position = funnel.find(".exception_class()")
if take_position < 0 or classify_position < 0 or take_position > classify_position:
    errors.append("exception funnel must consume payload before exception classification")

allowed_member_files = {
    root / "exec/error_handling.rs",
    root / "tests.rs",
}
for path in sorted(root.rglob("*.rs")):
    if path in allowed_member_files:
        continue
    text = without_comments(path.read_text())
    if re.search(r"\b(?:self|vm)\.pending_exception_payload\b", text):
        errors.append(f"{path}: direct canonical carrier access outside its reviewed adapters")

if errors:
    for error in errors:
        print(f"FAIL: {error}", file=sys.stderr)
    sys.exit(1)

print("OK: exception payloads use one keyed one-shot carrier (Issue #11647).")
PY
