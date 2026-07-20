#!/usr/bin/env bash
# check_name_based_lookup.sh - ratchet name-string lookup debt
# (Issues #10279 / #10459).
#
# The linked #10279 bug cluster came from several layers treating display names
# as identity: TypeVar binding/scope maps, runtime TypeVar identity caches, bare
# struct lookup, and method signature conversion through clobberable StructInfo
# maps. Issue #10459 turns that bug-cluster guard into the semantic-identity
# migration ratchet: existing legacy sites remain while owner-scoped IDs are
# introduced, but the counts must not grow silently. See
# docs/vm/SEMANTIC_IDENTITIES.md for the inventory and migration phases.
set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

python3 - <<'PY'
from pathlib import Path
import re
import sys


ROOT = Path(".")

CHECKS = [
    {
        "name": "typevar_scope_maps",
        "root": ROOT / "subset_julia_vm_types/src/inference_core",
        "pattern": re.compile(r"HashMap\s*<\s*String\s*,\s*CoreTypeVar\s*>"),
        "baseline": 0,
        "message": "TypeVar scope maps keyed only by name string",
    },
    {
        # RETIRED as an unclassified residual, 14 -> 0 (Issue #10992). The two
        # sanctioned name-to-CoreType boundaries are exact, private aliases:
        #   * `LexicalTypeBindings` — one dispatch candidate's source-level
        #     `where` substitution environment;
        #   * `RenderedTypeParseCache` — a pure complete-rendered-name parser
        #     memo shared by the ordinary and owner-preserving parse modes.
        #
        # The exact alias declarations are excluded below and separately held
        # as required anchors. Every other raw `HashMap<String, CoreType>` is
        # unclassified debt and must stay at zero. This detects same-count
        # substitution that the former baseline-14 ratchet could miss.
        "name": "typevar_core_bindings",
        "root": ROOT / "subset_julia_vm_types/src/inference_core",
        "pattern": re.compile(r"HashMap\s*<\s*String\s*,\s*CoreType\s*>"),
        "baseline": 0,
        "message": "unclassified TypeVar/CoreType maps keyed only by name string",
        "classified_lines": {
            (
                ROOT / "subset_julia_vm_types/src/inference_core/dispatch_resolver.rs",
                "type LexicalTypeBindings = HashMap<String, CoreType>;",
            ),
            (
                ROOT / "subset_julia_vm_types/src/inference_core/type_core.rs",
                "type RenderedTypeParseCache = std::cell::RefCell<HashMap<String, CoreType>>;",
            ),
        },
    },
    {
        "name": "lowering_binder_parallel_stacks",
        "root": ROOT / "subset_julia_vm_lowering/src/lowering",
        "pattern": re.compile(r"\b(?:EXCLUDED_PARAMS|active_type_params)\s*[:=]"),
        "baseline": 0,
        "message": "parallel lexical where-binder scope stacks outside type_binder_env",
    },
    {
        # RETIRED, 61 -> 0 (Issue #11078). `SharedCompileContext::struct_table`
        # is a `StructRegistry`: entries are keyed by the owner-scoped
        # `StructId { module: ModuleId, local }`, and names are aliases into
        # that id space (one `name -> StructId` index — the single lexical
        # resolution boundary property #3 of SEMANTIC_IDENTITIES.md sanctions).
        # `base_struct_table` is now `HashMap<String, StructId>`, an alias map
        # into the SAME id space, not a second table of struct layouts.
        # Hold this at 0: a new name-keyed `StructInfo` map is now a genuine
        # regression, not pre-existing debt.
        "name": "structinfo_name_maps_compile",
        "root": ROOT / "subset_julia_vm_compile/src/compile",
        "pattern": re.compile(r"HashMap\s*<\s*String\s*,\s*StructInfo\s*>"),
        "baseline": 0,
        "message": "compile StructInfo maps keyed only by name string",
    },
    {
        # RETIRED, 20 -> 0 (Issue #11046). `StructRegistry::resolve_scoped`
        # owns exact-qualified, current-module, Main/Base-origin, and lexical
        # alias ordering. The inference-only `StructTypeInfo` map is explicitly
        # classified behind `lookup_struct_type_info`; its rows carry `type_id`
        # and are field-layout projections, not declaration identity.
        "name": "struct_table_bare_gets_compile",
        "root": ROOT / "subset_julia_vm_compile/src/compile",
        "pattern": re.compile(
            r"\b(?:struct_table|base_struct_table)\.get\s*\(\s*(?:name|base_name)\s*\)"
        ),
        "baseline": 0,
        "message": "bare-name struct_table/base_struct_table lookups",
    },
    {
        "name": "runtime_typevar_identity_fields",
        "root": ROOT / "subset_julia_vm_vm/src/vm/mod.rs",
        "pattern": re.compile(
            r"runtime_typevar_identities\s*:\s*HashMap\s*<\s*\(\s*String\s*,\s*"
            r"Option\s*<\s*String\s*>\s*\)",
            re.MULTILINE | re.DOTALL,
        ),
        "baseline": 0,
        "message": "runtime TypeVar identity cache keyed by rendered name/bound strings",
        "multi_line": True,
    },
    {
        "name": "runtime_typevar_projection_identity_fields",
        "root": ROOT / "subset_julia_vm_vm/src/vm/mod.rs",
        "pattern": re.compile(
            r"runtime_typevar_projection_identities\s*:\s*HashMap\s*<\s*\(\s*"
            r"String\s*,\s*String\s*,\s*Option\s*<\s*String\s*>\s*\)",
            re.MULTILINE | re.DOTALL,
        ),
        "baseline": 0,
        "message": "runtime TypeVar projection cache keyed by rendered owner/name/bound strings",
        "multi_line": True,
    },
]

REQUIRED_ANCHORS = [
    (
        ROOT / "subset_julia_vm_types/src/inference_core/dispatch_resolver.rs",
        "type LexicalTypeBindings = HashMap<String, CoreType>;",
        "single-candidate lexical where-binding authority",
    ),
    (
        ROOT / "subset_julia_vm_types/src/inference_core/type_core.rs",
        "type RenderedTypeParseCache = std::cell::RefCell<HashMap<String, CoreType>>;",
        "rendered-name parser memo boundary",
    ),
    (
        ROOT / "subset_julia_vm_bytecode/src/struct_registry.rs",
        "by_owner_name: HashMap<(ModuleId, String), StructId>",
        "owner-name declaration index",
    ),
    (
        ROOT / "subset_julia_vm_bytecode/src/struct_registry.rs",
        "self.resolve_in_owner(owner_path, name)",
        "current-module owner recovery",
    ),
    (
        ROOT / "subset_julia_vm_bytecode/src/struct_registry.rs",
        "self.resolve_in_owner(MAIN_MODULE_PATH, name)",
        "Main-owner recovery",
    ),
    (
        ROOT / "subset_julia_vm_bytecode/src/struct_registry.rs",
        "pub fn insert_owned",
        "declaring-owner registration independent of display spelling",
    ),
    (
        ROOT / "subset_julia_vm_bytecode/src/struct_registry.rs",
        "pub fn insert_alias",
        "explicit lexical alias registration",
    ),
    (
        ROOT / "subset_julia_vm_bytecode/src/struct_registry.rs",
        "pub fn resolve_type_id",
        "deterministic legacy type-id projection",
    ),
    (
        ROOT / "subset_julia_vm_compile/src/compile/cache.rs",
        "restored_struct_owner(&def.name, &parametric_structs)",
        "cache-restored bare parametric owner recovery",
    ),
    (
        ROOT / "subset_julia_vm_compile/src/compile/cache.rs",
        "struct_table.insert_owned(",
        "cache-restored owner-aware declaration insertion",
    ),
    (
        ROOT / "subset_julia_vm_compile/src/compile/expr/struct_.rs",
        "struct_table.resolve_type_id(type_id)",
        "deterministic field-read layout projection",
    ),
    (
        ROOT / "subset_julia_vm_compile/src/compile/stmt.rs",
        "struct_table.resolve_type_id(type_id)",
        "deterministic field-write layout projection",
    ),
    (
        ROOT / "subset_julia_vm_compile/src/compile/abstract_interp/struct_info.rs",
        "pub fn lookup_struct_type_info",
        "classified inference-only field-layout projection",
    ),
]

FORBIDDEN_LEGACY_ANCHORS = [
    "base_struct_table",
    "base_origin_bare_names",
    "lookup_bare_struct_info",
    "julia_type_to_value_type_with_origin_table",
    "canonical_entries()",
]


def hits_for(root: Path, pattern: re.Pattern[str]) -> list[tuple[Path, int, str]]:
    if not root.exists():
        print(f"ERROR: expected audit root is missing: {root}", file=sys.stderr)
        sys.exit(1)

    hits: list[tuple[Path, int, str]] = []
    paths = [root] if root.is_file() else sorted(root.rglob("*.rs"))
    for path in paths:
        text = path.read_text(encoding="utf-8", errors="ignore")
        for lineno, line in enumerate(text.splitlines(), 1):
            if pattern.search(line):
                hits.append((path, lineno, line.strip()))
    return hits


def multi_line_hits_for(root: Path, pattern: re.Pattern[str]) -> list[tuple[Path, int, str]]:
    if not root.exists():
        print(f"ERROR: expected audit root is missing: {root}", file=sys.stderr)
        sys.exit(1)

    hits: list[tuple[Path, int, str]] = []
    paths = [root] if root.is_file() else sorted(root.rglob("*.rs"))
    for path in paths:
        text = path.read_text(encoding="utf-8", errors="ignore")
        for match in pattern.finditer(text):
            lineno = text.count("\n", 0, match.start()) + 1
            snippet = " ".join(match.group(0).split())
            hits.append((path, lineno, snippet))
    return hits


failed = False
for path, anchor, description in REQUIRED_ANCHORS:
    if not path.exists():
        print(f"ERROR: required #10279 anchor file is missing: {path}", file=sys.stderr)
        failed = True
        continue
    if anchor not in path.read_text(encoding="utf-8", errors="ignore"):
        print(
            f"FAIL: missing semantic-identity anchor for {description}: "
            f"{path} must contain `{anchor}`.",
            file=sys.stderr,
        )
        failed = True

compile_source = "\n".join(
    path.read_text(encoding="utf-8", errors="ignore")
    for path in sorted((ROOT / "subset_julia_vm_compile/src").rglob("*.rs"))
)
for anchor in FORBIDDEN_LEGACY_ANCHORS:
    if anchor in compile_source:
        print(
            f"FAIL: retired #11046 fallback anchor remains: `{anchor}`.",
            file=sys.stderr,
        )
        failed = True

for check in CHECKS:
    if check.get("multi_line"):
        hits = multi_line_hits_for(check["root"], check["pattern"])
    else:
        hits = hits_for(check["root"], check["pattern"])
    classified_lines = check.get("classified_lines", set())
    hits = [
        hit
        for hit in hits
        if (hit[0], hit[2]) not in classified_lines
    ]
    count = len(hits)
    baseline = check["baseline"]
    if count > baseline:
        failed = True
        print(
            f"FAIL: {check['name']} count grew from baseline {baseline} to {count} "
            f"(Issue #10279: {check['message']}).",
            file=sys.stderr,
        )
        print(
            "      Do not add new name-string identity lookups; use scoped TypeVar identity, "
            "module-aware struct lookup, or origin-aware signature conversion. "
            "See docs/vm/SEMANTIC_IDENTITIES.md.",
            file=sys.stderr,
        )
        for path, lineno, line in hits:
            print(f"      {path}:{lineno}: {line}", file=sys.stderr)
    elif count < baseline:
        print(
            f"OK: {check['name']} is below baseline ({count} < {baseline}); "
            "tighten the baseline in this script when retiring the debt."
        )
    else:
        print(f"OK: {check['name']} remains at baseline {baseline}.")

if failed:
    sys.exit(1)

print("OK: name-based lookup debt did not grow (Issues #10279 / #10459).")
PY
