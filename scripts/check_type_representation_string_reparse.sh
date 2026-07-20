#!/usr/bin/env bash
# check_type_representation_string_reparse.sh - ratchet semantic type-string
# reparsing in inference/dispatch/runtime type paths (Issues #10460, #11208).
#
# Structured UnionAll/TypeVar semantics must eventually flow through CoreType
# and owner-scoped identities, not by rendering JuliaType/CoreType to display
# strings and reparsing them. The current legacy sites are allowed while the
# migration is open, but new call sites in semantic inference/dispatch/runtime
# type roots must not land silently.
set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

python3 - <<'PY'
from pathlib import Path
import hashlib
import re
import sys

ROOT = Path(".")

ROOTS = {
    "types_inference_core": ROOT / "subset_julia_vm_types/src/inference_core",
    "compile_inference": ROOT / "subset_julia_vm_compile/src/compile/expr/infer",
    "compile_abstract_interp": ROOT / "subset_julia_vm_compile/src/compile/abstract_interp",
    "compile_dispatch": ROOT / "subset_julia_vm_compile/src/compile/expr/call",
    "vm_reflection": ROOT / "subset_julia_vm_vm/src/vm/builtins_reflection",
    "vm_type_objects": ROOT / "subset_julia_vm_vm/src/vm/type_objects.rs",
    "vm_type_utils": ROOT / "subset_julia_vm_vm/src/vm/type_utils.rs",
    "vm_builtins_types": ROOT / "subset_julia_vm_vm/src/vm/builtins_types.rs",
    "vm_exec": ROOT / "subset_julia_vm_vm/src/vm/exec",
}

PATTERNS = {
    # Match the semantic adapter symbol, not only one direct-call spelling.
    # This also catches aliases, multiline calls, and trivia before `(`.
    "julia_from_name": re.compile(r"\bJuliaType\s*::\s*from_name\b"),
    "julia_from_name_or_struct": re.compile(
        r"\bJuliaType\s*::\s*from_name_or_struct\b"
    ),
    "core_from_julia_name": re.compile(r"\bCoreType\s*::\s*from_julia_name\b"),
    "parametric_base_name": re.compile(r"\bparametric_base_name\b"),
    "julia_name_projection": re.compile(r"\.name\s*\(\s*\)"),
}

# Baselines captured from origin/main on 2026-07-12 and reset by Issue #10460
# to code tokens only. Symbol references include function-item aliases and
# multiline/comment-separated calls; comment and string contents are ignored.
# Lower these in the same PR that retires a site. Do not raise without an
# Issue-linked migration reason.
BASELINES = {
    ("types_inference_core", "julia_from_name"): 1,
    ("types_inference_core", "julia_from_name_or_struct"): 6,
    # 45 -> 22 (Issue #11205): tighten to the current structural CoreType
    # conversion surface after the intervening inference-core retirements.
    ("types_inference_core", "core_from_julia_name"): 24,
    ("types_inference_core", "parametric_base_name"): 0,
    ("types_inference_core", "julia_name_projection"): 2,
    ("compile_inference", "julia_from_name"): 3,
    # 15 -> 12 (Issue #10460): scalar inference and collect(::Tuple) project
    # through CoreType directly instead of rendering and reparsing JuliaType.
    ("compile_inference", "julia_from_name_or_struct"): 13,
    ("compile_inference", "core_from_julia_name"): 0,
    ("compile_inference", "parametric_base_name"): 0,
    ("compile_inference", "julia_name_projection"): 28,
    ("compile_abstract_interp", "julia_from_name"): 2,
    ("compile_abstract_interp", "julia_from_name_or_struct"): 1,
    ("compile_abstract_interp", "core_from_julia_name"): 0,
    ("compile_abstract_interp", "parametric_base_name"): 0,
    ("compile_abstract_interp", "julia_name_projection"): 5,
    ("compile_dispatch", "julia_from_name"): 0,
    # 2 -> 5 (Issue #11205): PR #11096 added three constructor-bound adapters
    # while completing parametric constructor-self parity.
    ("compile_dispatch", "julia_from_name_or_struct"): 5,
    ("compile_dispatch", "core_from_julia_name"): 3,
    ("compile_dispatch", "parametric_base_name"): 0,
    ("compile_dispatch", "julia_name_projection"): 4,
    ("vm_reflection", "julia_from_name"): 1,
    # 21 -> 23 (Issue #11205): PR #11022 added the two legacy-bound adapters
    # needed by the structural TypeVarProjectionKey migration.
    # 23 -> 24 (Issue #11507): the infer_return_type / return_types path for a
    # builtin numeric constructor (Int64/Float64/BigInt/BigFloat, which have no
    # pure-Julia wrapper) resolves the callee's return JuliaType from its
    # reflection callee NAME string — no structural JuliaType is available at
    # that reflection site. One more use of the established vm_reflection
    # helper; full structural migration remains the #10460 epic.
    ("vm_reflection", "julia_from_name_or_struct"): 24,
    ("vm_reflection", "core_from_julia_name"): 0,
    ("vm_reflection", "parametric_base_name"): 0,
    # 5 -> 7 (Issue #11205): PR #10864 preserves dependent runtime TypeVar
    # bounds when projecting the identity-bearing wrapper for reflection.
    # 7 -> 8 (Issue #11402): constructor reflection resolves the applied
    # parametric spelling structurally (`S{Int64}` constructs itself); the
    # projection builds that spelling's JuliaType for the reflection result.
    ("vm_reflection", "julia_name_projection"): 8,
    ("vm_type_objects", "julia_from_name"): 1,
    # 1 -> 4 (Issue #10460): the token scanner now strips raw strings and char
    # literals before cfg(test) brace tracking, exposing three production sites
    # that the old lexical pass accidentally hid.
    ("vm_type_objects", "julia_from_name_or_struct"): 4,
    ("vm_type_objects", "core_from_julia_name"): 0,
    # 9 -> 0 (Issue #10460): RuntimeTypeHandle reads owner-qualified base and
    # parameters from its canonical CoreType identity.
    ("vm_type_objects", "parametric_base_name"): 0,
    # 17 -> 18 (Issue #8451): owner-qualified TypeName identity centralizes one
    # projection site here; the paired vm_exec ratchet below keeps the total
    # projection surface flat.
    # 4 -> 10 (Issue #10460): same corrected lexical inventory as above; this
    # is an audit coverage increase, not new projection debt.
    ("vm_type_objects", "julia_name_projection"): 10,
    ("vm_type_utils", "julia_from_name"): 0,
    ("vm_type_utils", "julia_from_name_or_struct"): 0,
    ("vm_type_utils", "core_from_julia_name"): 0,
    ("vm_type_utils", "parametric_base_name"): 0,
    # 2 -> 0 (Issue #10460): VectorOf/MatrixOf share one structural TypeVar
    # extractor, and runtime UnionAll construction now rebinds builtin-shadow
    # source binders through CoreType before alias comparison.
    ("vm_type_utils", "julia_name_projection"): 0,
    ("vm_builtins_types", "julia_from_name"): 7,
    ("vm_builtins_types", "julia_from_name_or_struct"): 4,
    # 2 -> 0 (Issue #10460): _typeintersect consumes runtime JuliaType values,
    # computes on CoreType, and projects the result structurally.
    ("vm_builtins_types", "core_from_julia_name"): 0,
    ("vm_builtins_types", "parametric_base_name"): 0,
    ("vm_builtins_types", "julia_name_projection"): 16,
    ("vm_exec", "julia_from_name"): 4,
    # 19 -> 18: tighten to the current surface after the constructor/runtime
    # owner-resolution work retired one legacy adapter.
    # 18 -> 19 (Issue #11555): the ConstructParametricType validation path
    # builds a `DataType` from the parametric application's base-type name
    # (`expected_name`) to raise the correct TypeError for an invalid
    # type-parameter value — the runtime construction site inherently works
    # with the base name string, no structural JuliaType is available there.
    # One more use of the established vm_exec helper; full structural
    # migration remains the #10460 epic.
    ("vm_exec", "julia_from_name_or_struct"): 19,
    ("vm_exec", "core_from_julia_name"): 1,
    ("vm_exec", "parametric_base_name"): 0,
    # 37 -> 36 (Issue #8451): one owner-qualified TypeName identity projection
    # moved into vm_type_objects; the paired ratchet above keeps the cross-root
    # total flat.
    ("vm_exec", "julia_name_projection"): 36,
}

# SHA-256 of the sorted `path<TAB>normalized source line` multiset for every
# bucket above. Counts catch additions; these fingerprints also catch a
# same-count substitution of one reviewed boundary adapter for a new semantic
# adapter. Source movement is harmless because line numbers are excluded.
SITE_INVENTORY_DIGESTS = {
    ("types_inference_core", "julia_from_name"): "37c32b599f04d8652dff992151d809b4b8264bce7feebb1691818233214b7cf8",
    ("types_inference_core", "julia_from_name_or_struct"): "b207af4442c0aa718e843c51ab8af6de2a17448282dd23924f084b3df65d27e9",
    ("types_inference_core", "core_from_julia_name"): "63077be80de79dc8eb456c4be993f46484238c496aed2fd99e86c6d2446f70c2",
    ("types_inference_core", "parametric_base_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("types_inference_core", "julia_name_projection"): "925eb3dc22321ff686cbf8d8cf245d97b1a5dbafe03e65f1f240883843c7b574",
    ("compile_inference", "julia_from_name"): "42c8bbdae2be5b473383dbfc095a340c1b37ef9843065d4ae3566a3a41e98f58",
    ("compile_inference", "julia_from_name_or_struct"): "804144e07b411044064eff89d170e8a635365b1abb8b1f059bb43d03d845fc18",
    ("compile_inference", "core_from_julia_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("compile_inference", "parametric_base_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("compile_inference", "julia_name_projection"): "59636a34d0e755e30501a6fc4af937e3740b6c0eb96271913f4010ac57fcc4e7",
    ("compile_abstract_interp", "julia_from_name"): "2e6186b2da3aa5a26b8615d49af9d07223942c49ecce68775c70b9d338288453",
    ("compile_abstract_interp", "julia_from_name_or_struct"): "97e42eee827e66262ea1e3e23a748abe590281ce38a5dc7a78db52c5d7358a07",
    ("compile_abstract_interp", "core_from_julia_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("compile_abstract_interp", "parametric_base_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("compile_abstract_interp", "julia_name_projection"): "5d4050bcb2202c6b8c247a3f2a957d0c876b9b54d5353f3815c21096810353ce",
    ("compile_dispatch", "julia_from_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("compile_dispatch", "julia_from_name_or_struct"): "05168bebc04c5c8b93c130d1988ddf2ec0ea0f7e9e49c8ae7ef2e73097659d52",
    ("compile_dispatch", "core_from_julia_name"): "6d5244b6fa93fdf3a2b5e75f6a10f1c9d0e49c230af415bcd13ac3a5fc9789ea",
    ("compile_dispatch", "parametric_base_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("compile_dispatch", "julia_name_projection"): "de6c79da2e26b5c8bd34585df1ec9fdc1106123d0b07c99dcb378b4a14a9e606",
    ("vm_reflection", "julia_from_name"): "9bbe3efa6097f63e27447f1b4abd6137bdaf5d1306a4502f0f6502b2a7109c23",
    ("vm_reflection", "julia_from_name_or_struct"): "2305f6d576ba223a2205f52d52e7813f84753b241c4b78c4bde2217dc1239952",
    ("vm_reflection", "core_from_julia_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("vm_reflection", "parametric_base_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("vm_reflection", "julia_name_projection"): "dd3b13da6b418e374b7445fbaae8e865931cb89c2d8db5ba9d4137fdc310f4a8",
    ("vm_type_objects", "julia_from_name"): "b7fae0fc27215c198f75b8495efd2c269cdfecf07ed18a4b8dd8bff9b4962631",
    ("vm_type_objects", "julia_from_name_or_struct"): "1040e24f205925e5dd0fcd2bafa448a4848e301a265c8d4e5c9b096ecaca8ff3",
    ("vm_type_objects", "core_from_julia_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("vm_type_objects", "parametric_base_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("vm_type_objects", "julia_name_projection"): "fed8c3971b879a1cc92283a96f621cf5b27287b8b8696f488a2bd76ccb98b2cc",
    ("vm_type_utils", "julia_from_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("vm_type_utils", "julia_from_name_or_struct"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("vm_type_utils", "core_from_julia_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("vm_type_utils", "parametric_base_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("vm_type_utils", "julia_name_projection"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("vm_builtins_types", "julia_from_name"): "29f758c1daa9c987f920acd3192492c0c0207af07ec17b162dd362a77cc04587",
    ("vm_builtins_types", "julia_from_name_or_struct"): "d06106bdc7590e10def8b86b04bef4ddddbe30cdea176e5d580e17a8d5b58ac5",
    ("vm_builtins_types", "core_from_julia_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("vm_builtins_types", "parametric_base_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("vm_builtins_types", "julia_name_projection"): "c0c53b7ac9509034b61f61d4286aad667c6966d11fc55c667612d3512fee86ec",
    ("vm_exec", "julia_from_name"): "959ab68b47935b3ee7bfaeed487a34efc401ff24a57e60c6b4552b55f14cf4b1",
    ("vm_exec", "julia_from_name_or_struct"): "5a3254a60e504ad60fcdcf679c8688509f7a3133805b8b93056a232aafe7eb84",
    ("vm_exec", "core_from_julia_name"): "a147a1f30b13cb2646f0dec60e9c57ab03a8fe57eaf8db81bd45d2b2788895bf",
    ("vm_exec", "parametric_base_name"): "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    ("vm_exec", "julia_name_projection"): "c37da6aadcf9934239899937d02cc683bdc270fd5706433d98acfb9489394743",
}


def strip_rust_comments_and_strings(text: str) -> str:
    """Blank comments and ordinary string literals while preserving newlines."""
    out: list[str] = []
    index = 0
    block_depth = 0
    in_string = False
    escaped = False
    while index < len(text):
        char = text[index]
        following = text[index + 1] if index + 1 < len(text) else ""
        raw_prefix = (
            re.match(r"(?:br|r)(?P<hashes>#{0,255})\"", text[index:])
            if char in {"b", "r"}
            else None
        )
        if not block_depth and not in_string and raw_prefix:
            opener = raw_prefix.group(0)
            terminator = '"' + raw_prefix.group("hashes")
            end = text.find(terminator, index + len(opener))
            end = len(text) if end < 0 else end + len(terminator)
            out.extend("\n" if source_char == "\n" else " " for source_char in text[index:end])
            index = end
            continue
        char_literal = (
            re.match(
                r"'(?:\\(?:u\{[0-9A-Fa-f_]+\}|x[0-9A-Fa-f]{2}|.)|[^\\'\n])'",
                text[index:],
            )
            if char == "'"
            else None
        )
        if not block_depth and not in_string and char_literal:
            literal = char_literal.group(0)
            out.extend(" " for _ in literal)
            index += len(literal)
            continue
        if block_depth:
            if char == "/" and following == "*":
                block_depth += 1
                out.extend((" ", " "))
                index += 2
            elif char == "*" and following == "/":
                block_depth -= 1
                out.extend((" ", " "))
                index += 2
            else:
                out.append("\n" if char == "\n" else " ")
                index += 1
            continue
        if in_string:
            out.append("\n" if char == "\n" else " ")
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            index += 1
            continue
        if char == "/" and following == "/":
            while index < len(text) and text[index] != "\n":
                out.append(" ")
                index += 1
            continue
        if char == "/" and following == "*":
            block_depth = 1
            out.extend((" ", " "))
            index += 2
            continue
        if char == '"':
            in_string = True
            out.append(" ")
            index += 1
            continue
        out.append(char)
        index += 1
    return "".join(out)


def strip_cfg_test_modules(text: str) -> str:
    """Blank test modules while preserving line numbers and pending cfg trivia.

    Blank lines and adjacent attributes may separate `#[cfg(test)]` from
    `mod tests`; a real non-module item clears the pending marker. The focused
    matrix below pins both sides of that lightweight scanner contract.
    """
    out: list[str] = []
    pending_cfg_test = False
    skipping = False
    depth = 0

    for line in text.splitlines():
        stripped = line.strip()
        if not skipping and stripped.startswith("#[cfg(test)]"):
            pending_cfg_test = True
            out.append("")
            continue

        if pending_cfg_test and not skipping:
            if re.match(r"(pub\s+)?mod\s+tests\b", stripped):
                skipping = True
                depth = line.count("{") - line.count("}")
                if depth <= 0:
                    depth = 1
                out.append("")
                continue
            # Rustfmt commonly leaves a blank line (and may leave another
            # attribute) between `#[cfg(test)]` and `mod tests`. Preserve the
            # pending marker across those trivia lines; clearing it here made
            # the audit count old test-only reparses as new production debt
            # after harmless formatting changes (Issue #11205).
            if not stripped or stripped.startswith("#["):
                out.append("")
                continue
            pending_cfg_test = False

        if skipping:
            depth += line.count("{") - line.count("}")
            out.append("")
            if depth <= 0:
                skipping = False
            continue

        out.append(line)

    return "\n".join(out)


def adapter_pattern(pattern_name: str, text: str) -> re.Pattern[str]:
    """Build a trivia-tolerant symbol pattern including local type aliases."""
    owner_and_member = {
        "julia_from_name": ("JuliaType", "from_name"),
        "julia_from_name_or_struct": ("JuliaType", "from_name_or_struct"),
        "core_from_julia_name": ("CoreType", "from_julia_name"),
    }.get(pattern_name)
    if owner_and_member is None:
        return PATTERNS[pattern_name]

    owner, member = owner_and_member
    aliases = {owner}
    aliases.update(
        re.findall(rf"\b{owner}\s+as\s+([A-Za-z_][A-Za-z0-9_]*)\b", text)
    )
    aliases.update(
        re.findall(
            rf"\btype\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*"
            rf"(?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*)*{owner}\b",
            text,
        )
    )
    owners = "|".join(re.escape(alias) for alias in sorted(aliases))
    direct_owner = rf"\b(?:{owners})"
    # Rust UFCS permits both `<JuliaType>::adapter` and a fully-qualified
    # `<crate::types::JuliaType>::adapter`. Treat those as the same semantic
    # adapter boundary as the ordinary `JuliaType::adapter` spelling so a
    # wrapper cannot hide a new render/reparse bridge from the exact inventory.
    ufcs_owner = (
        rf"<\s*(?:(?:[A-Za-z_][A-Za-z0-9_]*)\s*::\s*)*"
        rf"(?:{owners})\s*>"
    )
    return re.compile(rf"(?:{direct_owner}|{ufcs_owner})\s*::\s*{member}\b")


def matching_lines(text: str, pattern_name: str) -> list[tuple[int, str]]:
    """Return one normalized hit per source line, including multiline paths."""
    pattern = adapter_pattern(pattern_name, text)
    lines = text.splitlines()
    hits: list[tuple[int, str]] = []
    seen_lines: set[int] = set()
    for match in pattern.finditer(text):
        lineno = text.count("\n", 0, match.start()) + 1
        if lineno in seen_lines:
            continue
        seen_lines.add(lineno)
        source_line = lines[lineno - 1].strip() if lineno <= len(lines) else ""
        normalized_match = re.sub(r"\s+", " ", match.group(0)).strip()
        hits.append((lineno, source_line or normalized_match))
    return hits


def check_cfg_test_trivia_contract() -> None:
    """Keep test-only debt ignored without hiding the same production token."""
    test_module_cases = {
        "immediate": """#[cfg(test)]
mod tests {
    fn ignored() { let _ = JuliaType::from_name(\"Immediate\"); }
}
""",
        "blank-line": """#[cfg(test)]

mod tests {
    fn ignored() { let _ = JuliaType::from_name(\"BlankLine\"); }
}
""",
        "adjacent-attribute": """#[cfg(test)]
#[allow(dead_code)]
mod tests {
    fn ignored() { let _ = JuliaType::from_name(\"AdjacentAttribute\"); }
}
""",
        "string-and-comment-braces": """#[cfg(test)]
mod tests {
    const OPEN: &str = "{";
    const RAW: &str = r#"{"#;
    const CHAR: char = '{';
    // An unmatched { in trivia must not extend the skipped module.
}
fn production_path() { let _ = JuliaType::from_name("Visible"); }
""",
    }
    for layout, source in test_module_cases.items():
        stripped = strip_cfg_test_modules(strip_rust_comments_and_strings(source))
        hits = matching_lines(stripped, "julia_from_name")
        expected_hits = 1 if layout == "string-and-comment-braces" else 0
        if len(hits) != expected_hits:
            print(
                "FAIL: cfg(test) trivia self-test leaked test-only token or hid "
                "production code for "
                f"the {layout} layout (Issue #11208).",
                file=sys.stderr,
            )
            sys.exit(1)

    non_leaking_source = """#[cfg(test)]
fn test_only_helper() {}

fn production_path() {
    let _ = JuliaType::from_name(\"ProductionMustRemainVisible\");
}
"""
    stripped = strip_cfg_test_modules(strip_rust_comments_and_strings(non_leaking_source))
    visible = matching_lines(stripped, "julia_from_name")
    if len(visible) != 1:
        print(
            "FAIL: cfg(test) trivia self-test hid a production type-string "
            "reparse after a non-module item (Issue #11208).",
            file=sys.stderr,
        )
        sys.exit(1)


def hits_for(root: Path, pattern_name: str) -> list[tuple[Path, int, str]]:
    if not root.exists():
        print(f"ERROR: expected audit root is missing: {root}", file=sys.stderr)
        sys.exit(1)

    hits: list[tuple[Path, int, str]] = []
    paths = [root] if root.is_file() else sorted(root.rglob("*.rs"))
    for path in paths:
        text = path.read_text(encoding="utf-8", errors="ignore")
        text = strip_cfg_test_modules(strip_rust_comments_and_strings(text))
        hits.extend(
            (path, lineno, line)
            for lineno, line in matching_lines(text, pattern_name)
        )
    return hits


def site_inventory_digest(hits: list[tuple[Path, int, str]]) -> str:
    sites = sorted(f"{path.as_posix()}\t{line}" for path, _, line in hits)
    payload = "\n".join(sites) + "\n"
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


def check_semantic_adapter_pattern_contract() -> None:
    cases = {
        "julia_from_name": [
            "let parse = JuliaType::from_name; let ty = parse(name);",
            "let ty = JuliaType::from_name /* reviewed? */ (name);",
            "use crate::types::JuliaType as JT; let ty = JT::from_name(name);",
            "let ty = JuliaType /* trivia */ :: from_name(name);",
            "let ty = JuliaType::\nfrom_name(name);",
        ],
        "julia_from_name_or_struct": [
            "let parse = JuliaType::from_name_or_struct;",
            "let ty = <JuliaType>::from_name_or_struct(name);",
            "let ty = <crate::types::JuliaType>::from_name_or_struct(name);",
        ],
        "core_from_julia_name": [
            "let ty = CoreType::from_julia_name\n(name);",
        ],
        "parametric_base_name": [
            "let parse = parametric_base_name;",
        ],
    }
    for pattern_name, sources in cases.items():
        for source in sources:
            stripped = strip_rust_comments_and_strings(source)
            if not matching_lines(stripped, pattern_name):
                print(
                    f"FAIL: {pattern_name} audit pattern missed a valid Rust "
                    "reference spelling (Issue #10460).",
                    file=sys.stderr,
                )
                sys.exit(1)

    lexical_source = """// JuliaType::from_name(comment_only)
let ignored = \"CoreType::from_julia_name(string_only)\";
let parse = JuliaType::from_name /* trivia */;
"""
    stripped = strip_rust_comments_and_strings(lexical_source)
    if matching_lines(stripped, "core_from_julia_name"):
        print(
            "FAIL: code-token scanner retained a string-literal adapter mention "
            "(Issue #10460).",
            file=sys.stderr,
        )
        sys.exit(1)
    julia_hits = matching_lines(stripped, "julia_from_name")
    if len(julia_hits) != 1:
        print(
            "FAIL: code-token scanner did not isolate the one production alias "
            "from comment/string mentions (Issue #10460).",
            file=sys.stderr,
        )
        sys.exit(1)

    wrapper_source = """
fn parse_projected(s: &str) -> JuliaType {
    <JuliaType>::from_name_or_struct(s)
}

fn cold_projection(ty: &JuliaType) -> JuliaType {
    parse_projected(&ty.to_string())
}
"""
    wrapper_hits = matching_lines(
        strip_rust_comments_and_strings(wrapper_source),
        "julia_from_name_or_struct",
    )
    if len(wrapper_hits) != 1:
        print(
            "FAIL: semantic adapter boundary audit missed a UFCS adapter behind "
            "a wrapper invocation (Issue #10460).",
            file=sys.stderr,
        )
        sys.exit(1)


check_cfg_test_trivia_contract()
check_semantic_adapter_pattern_contract()

failed = False
for root_name, root in ROOTS.items():
    for pattern_name in PATTERNS:
        baseline = BASELINES[(root_name, pattern_name)]
        expected_digest = SITE_INVENTORY_DIGESTS[(root_name, pattern_name)]
        hits = hits_for(root, pattern_name)
        count = len(hits)
        digest = site_inventory_digest(hits)
        label = f"{root_name}/{pattern_name}"
        if count > baseline:
            failed = True
            print(
                f"FAIL: {label} count grew from baseline {baseline} to {count} "
                "(Issue #10460: semantic type-string reparsing must not grow).",
                file=sys.stderr,
            )
            print(
                "      Prefer structured CoreType/TypeExpr/owner-scoped identity paths; "
                "if a temporary string bridge is unavoidable, document and ratchet it.",
                file=sys.stderr,
            )
            for path, lineno, line in hits:
                print(f"      {path}:{lineno}: {line}", file=sys.stderr)
            print(f"      observed digest: {digest}", file=sys.stderr)
        elif count < baseline:
            failed = True
            print(
                f"FAIL: {label} is below baseline ({count} < {baseline}); its exact "
                "inventory is no longer certified (Issue #10460).",
                file=sys.stderr,
            )
            print(
                "      Review the removed and remaining sites, then tighten both the "
                "baseline and inventory digest in this script.",
                file=sys.stderr,
            )
            for path, lineno, line in hits:
                print(f"      {path}:{lineno}: {line}", file=sys.stderr)
            print(f"      observed digest: {digest}", file=sys.stderr)
        elif digest != expected_digest:
            failed = True
            print(
                f"FAIL: {label} exact site inventory changed without changing "
                f"its count (Issue #10460).",
                file=sys.stderr,
            )
            print(
                "      Review every replacement as a semantic boundary change, then "
                "update the inventory digest and its Issue-linked documentation.",
                file=sys.stderr,
            )
            for path, lineno, line in hits:
                print(f"      {path}:{lineno}: {line}", file=sys.stderr)
            print(f"      observed digest: {digest}", file=sys.stderr)
        else:
            print(f"OK: {label} remains at baseline {baseline} with exact inventory.")

if failed:
    sys.exit(1)

print("OK: type-representation string reparse debt did not grow (Issue #10460).")
PY
