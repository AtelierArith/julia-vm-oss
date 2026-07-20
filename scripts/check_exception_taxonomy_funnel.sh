#!/usr/bin/env bash
# check_exception_taxonomy_funnel.sh — the exception-type taxonomy funnel guard
# (Issue #11146, Phase 2a of the #10813 epic).
#
# WHY THIS EXISTS
#
# sjulia's exception classes used to be chosen ad hoc at each raise site: a site
# picked the "nearest" `VmError` variant and then wrote whatever message text it
# liked. Issue #10354's fixture-fallout measurement found that FOUR of its five
# root causes were literally one shape —
#
#     VmError::TypeError(format!("ArgumentError: {} ...", name))
#
# — a raise whose MESSAGE said `ArgumentError` while the raised VARIANT (and so
# `typeof(caught)`) was `TypeError`. An `isa`-dispatching `catch` block (the
# upstream-idiomatic pattern) silently took the wrong branch, and `@test_throws`
# never checked the type, so nothing failed.
#
# Issue #11146 made the exception *object's* class structural: it is derived from
# ONE compile-time-exhaustive match, `VmError::exception_class()`
# (`subset_julia_vm_bytecode/src/error.rs`), which also derives catchability.
# What a compiler cannot check is the free-form message `String`. That residue is
# what this audit closes:
#
#   R1  no `VmError::<Variant>(...)` may carry a message that OPENS with the name
#       of a Julia exception class that contradicts the variant's own class in
#       the funnel;
#   R2  `vm_error_to_exception_value` (the catch-time exception builder) may not
#       hard-code a Julia exception struct-name literal — the name must come from
#       `ExceptionClass::julia_name()`, or a raise site could resurrect a
#       per-site class choice;
#   R3  the funnel's own match may not gain a `_ =>` catch-all arm, and
#       `is_catchable_vm_error` must delegate to it rather than re-listing
#       variants — the pre-#11146 hand-synced list is exactly how the two drifted;
#   R4  the PURE-JULIA layer (subset_julia_vm/src/julia/**.jl) may not raise a
#       class by naming it in an `error("<Class>: ...")` message — that throws an
#       `ErrorException`, so `typeof(caught)` contradicts the message exactly as
#       in R1. `throw(<Class>(...))` is the correct form. The message-only classes
#       were retired in #11146; the classes that need constructor arguments
#       (BoundsError/DomainError/InexactError) are ratcheted against
#       docs/vm/EXCEPTION_TAXONOMY_JULIA_BASELINE.tsv so the count can only
#       shrink.
#
# The audit derives the variant -> class table BY PARSING the funnel itself, so
# it can never disagree with the code it guards.
#
# Usage (from the repository root):
#   bash scripts/check_exception_taxonomy_funnel.sh
#   bash scripts/check_exception_taxonomy_funnel.sh --root <sandbox-root>   # self-test
#
# Exit code: 0 = clean, 1 = at least one violation (each printed with file:line).
#
# Dependencies: python3 (stdlib only), bash 3.2+.

set -uo pipefail

ROOT="."
if [ "${1:-}" = "--root" ]; then
    ROOT="${2:?--root needs a path}"
    shift 2
fi
if [ "$#" -ne 0 ]; then
    echo "FAIL: unknown argument(s): $*" >&2
    exit 2
fi

cd "$(dirname "$0")/.." || exit 1
cd "$ROOT" || exit 1

if [ ! -f subset_julia_vm_bytecode/src/error.rs ]; then
    echo "FAIL: check_exception_taxonomy_funnel.sh cannot find subset_julia_vm_bytecode/src/error.rs" >&2
    echo "      (the audit target moved; repoint the audit instead of letting it pass vacuously)" >&2
    exit 1
fi

python3 - "$PWD" <<'PY'
import os
import re
import sys

root = sys.argv[1]
FUNNEL = os.path.join(root, "subset_julia_vm_bytecode/src/error.rs")
BUILDER = os.path.join(root, "subset_julia_vm_vm/src/vm/exec/error_handling.rs")

failures = []

# --------------------------------------------------------------------------
# Parse the funnel: VmError variant -> ExceptionClass. The audit NEVER carries
# its own copy of this table; it reads the one the VM uses.
# --------------------------------------------------------------------------
funnel_src = open(FUNNEL, encoding="utf-8").read()

body = funnel_src.split("pub fn exception_class(&self) -> ExceptionClass {", 1)
if len(body) != 2:
    print("FAIL: R3 the funnel VmError::exception_class() is missing from "
          "subset_julia_vm_bytecode/src/error.rs -- the exception taxonomy has no "
          "single authority")
    sys.exit(1)
# The match body ends at the function's closing brace (first line that is exactly
# 4-space-indented `}` following the fn).
funnel_body = []
for line in body[1].splitlines():
    if line.rstrip() == "    }":
        break
    funnel_body.append(line)
funnel_body = "\n".join(funnel_body)

# R3a: no catch-all arm may weaken the exhaustiveness.
for m in re.finditer(r"^\s*_\s*=>", funnel_body, re.MULTILINE):
    failures.append(
        "R3 catch-all arm in VmError::exception_class(): a `_ =>` arm means a new "
        "VmError variant can be added WITHOUT declaring its Julia exception class, "
        "which is precisely the ad-hoc-taxonomy hole Issue #11146 closed "
        "(subset_julia_vm_bytecode/src/error.rs)"
    )

variant_class = {}
# Arms look like:  Self::A(_) | Self::B { .. } => ExceptionClass::Foo,
for arm in re.finditer(
    r"((?:Self::\w+(?:\([^)]*\)|\s*\{[^}]*\})?\s*\|\s*)*Self::\w+(?:\([^)]*\)|\s*\{[^}]*\})?)\s*=>\s*ExceptionClass::(\w+)",
    funnel_body,
):
    cls = arm.group(2)
    for variant in re.findall(r"Self::(\w+)", arm.group(1)):
        variant_class[variant] = cls

if len(variant_class) < 10:
    failures.append(
        "R3 could not parse the VmError -> ExceptionClass arms out of the funnel "
        "(parsed %d); the audit would pass vacuously, so it fails instead "
        "(Issue #11146 / #9129 failure mode F2)" % len(variant_class)
    )

classes = sorted(
    set(re.findall(r"Self::(\w+) => \"(?:\w+)\"", funnel_src))
    or set(variant_class.values())
)
# The Julia exception class NAMES (what a message must not contradict).
class_names = sorted(set(re.findall(r'Self::\w+ => "(\w+)",', funnel_src)))
if len(class_names) < 10:
    failures.append(
        "R3 could not parse ExceptionClass::julia_name()'s class names (parsed %d)"
        % len(class_names)
    )

# --------------------------------------------------------------------------
# R1: a raise site's message must not open with a class name that contradicts
# the variant it raises.
# --------------------------------------------------------------------------
CRATES = [
    "subset_julia_vm/src",
    "subset_julia_vm_lowering/src",
    "subset_julia_vm_compile/src",
    "subset_julia_vm_vm/src",
    "subset_julia_vm_bytecode/src",
    "subset_julia_vm_types/src",
    "subset_julia_vm_ir/src",
    "subset_julia_vm_ffi/src",
    "subset_julia_vm_parser/src",
    "subset_julia_vm_runtime/src",
    "subset_julia_vm_web/src",
]

def strip_comments(line, marker="//"):
    """Drop line comments so a comment that *quotes* the bad shape (this audit's
    own doc comments, and the fix-site comments explaining what the old code did)
    is not mistaken for the bad shape itself. String literals are respected, so a
    `#` or `//` inside a message is not treated as a comment.

    `marker` is `//` for Rust and `#` for Julia — scanning a .jl file with the
    Rust marker made this audit count its OWN explanatory comments as raise
    sites (caught by the range.jl logrange fix)."""
    out, in_str, escaped = [], False, False
    i = 0
    while i < len(line):
        ch = line[i]
        if in_str:
            out.append(ch)
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == '"':
                in_str = False
        else:
            if ch == '"':
                in_str = True
                out.append(ch)
            elif line.startswith(marker, i):
                break
            else:
                out.append(ch)
        i += 1
    return "".join(out)

construct_re = re.compile(r"VmError::(\w+)\s*\(")

for crate in CRATES:
    crate_path = os.path.join(root, crate)
    for dirpath, _dirnames, filenames in os.walk(crate_path):
        for filename in sorted(filenames):
            if not filename.endswith(".rs"):
                continue
            path = os.path.join(dirpath, filename)
            rel = os.path.relpath(path, root)
            lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
            code = [strip_comments(line) for line in lines]
            for idx, line in enumerate(code):
                m = construct_re.search(line)
                if not m:
                    continue
                variant = m.group(1)
                own_class = variant_class.get(variant)
                if own_class is None:
                    # Not a constructed variant (e.g. a `VmError::Foo => ...`
                    # match arm in a lookup table) or an unclassified name.
                    continue
                # The message literal may be on this line or the next two
                # (a `format!(` opener typically wraps).
                window = " ".join(code[idx : idx + 3])
                for literal in re.findall(r'"((?:[^"\\]|\\.)*)"', window):
                    head = re.match(r"(\w+): ", literal)
                    if not head:
                        continue
                    named = head.group(1)
                    if named not in class_names:
                        continue
                    if named == own_class:
                        continue
                    failures.append(
                        f"R1 {rel}:{idx + 1}: VmError::{variant} (funnel class "
                        f"{own_class}) carries a message that opens with "
                        f'"{named}: " -- the message names one exception class '
                        f"while typeof(caught) is another. Raise "
                        f"VmError-of-class-{named} instead of embedding the name "
                        f"in the text (Issue #11146; this exact shape was 4 of the "
                        f"5 root causes in Issue #10354)"
                    )

# --------------------------------------------------------------------------
# R2: the catch-time exception builder must take the struct name from the
# funnel, never from a hard-coded literal.
# --------------------------------------------------------------------------
if not os.path.exists(BUILDER):
    failures.append(
        "R2 subset_julia_vm_vm/src/vm/exec/error_handling.rs is missing -- the "
        "exception builder moved; repoint this audit instead of letting it pass "
        "vacuously (Issue #9129 failure mode F2)"
    )
else:
    builder_src = open(BUILDER, encoding="utf-8").read()
    fn_split = builder_src.split("fn vm_error_to_exception_value(", 1)
    if len(fn_split) != 2:
        failures.append(
            "R2 vm_error_to_exception_value() not found in error_handling.rs -- the "
            "catch-time exception builder moved; repoint this audit"
        )
    else:
        fn_body = []
        depth = 0
        started = False
        for line in fn_split[1].splitlines():
            depth += line.count("{") - line.count("}")
            if "{" in line:
                started = True
            fn_body.append(line)
            if started and depth <= 0:
                break
        fn_body = "\n".join(strip_comments(line) for line in fn_body)
        if "exception_class()" not in fn_body:
            failures.append(
                "R2 vm_error_to_exception_value() no longer derives the exception's "
                "class from the funnel (VmError::exception_class()) -- the struct "
                "name must not be re-decided per arm (Issue #11146)"
            )
        for cls in class_names:
            for m in re.finditer(r'"%s"' % re.escape(cls), fn_body):
                failures.append(
                    f'R2 vm_error_to_exception_value() hard-codes the exception '
                    f'struct-name literal "{cls}" -- the name must come from '
                    f"ExceptionClass::julia_name(), or a raise site can again bind a "
                    f"catch value whose class contradicts its variant (Issue #11146)"
                )
        # R3b: catchability must be derived, not re-listed.
        catchable_split = builder_src.split("fn is_catchable_vm_error(", 1)
        if len(catchable_split) == 2:
            head = catchable_split[1][:400]
            if "is_catchable()" not in head:
                failures.append(
                    "R3 is_catchable_vm_error() no longer delegates to the funnel "
                    "(VmError::is_catchable) -- a second, hand-maintained variant "
                    "list is how catchability and the exception object drifted apart "
                    "before Issue #11146"
                )

# --------------------------------------------------------------------------
# R4: the pure-Julia layer must not name an exception class inside an
# `error("<Class>: ...")` message (that raises ErrorException). Ratcheted:
# counts may shrink, never grow, and no new file may appear.
# --------------------------------------------------------------------------
BASELINE = os.path.join(root, "docs/vm/EXCEPTION_TAXONOMY_JULIA_BASELINE.tsv")
JULIA_ROOT = os.path.join(root, "subset_julia_vm/src/julia")

if os.path.isdir(JULIA_ROOT):
    if not os.path.exists(BASELINE):
        failures.append(
            "R4 docs/vm/EXCEPTION_TAXONOMY_JULIA_BASELINE.tsv is missing -- the "
            "Julia-layer ratchet has no baseline, so the divergence count could "
            "grow silently (Issue #11146)"
        )
        baseline = {}
    else:
        baseline = {}
        for line in open(BASELINE, encoding="utf-8"):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            cols = line.split("\t")
            if len(cols) >= 2:
                baseline[cols[0]] = int(cols[1])

    error_class_re = re.compile(r'error\("(%s):' % "|".join(class_names))
    observed = {}
    for dirpath, _dirnames, filenames in os.walk(JULIA_ROOT):
        for filename in sorted(filenames):
            if not filename.endswith(".jl"):
                continue
            path = os.path.join(dirpath, filename)
            rel = os.path.relpath(path, root)
            hits = 0
            for line in open(path, encoding="utf-8", errors="replace").read().splitlines():
                if error_class_re.search(strip_comments(line, "#")):
                    hits += 1
            if hits:
                observed[rel] = hits

    for rel, count in sorted(observed.items()):
        allowed = baseline.get(rel)
        if allowed is None:
            failures.append(
                f"R4 {rel}: {count} new `error(\"<Class>: ...\")` raise(s) -- this "
                f"throws an ErrorException whose MESSAGE names a different exception "
                f"class, so `typeof(caught)` contradicts it (the R1 defect, one layer "
                f"up). Use `throw(<Class>(...))` (Issue #11146)"
            )
        elif count > allowed:
            failures.append(
                f"R4 {rel}: `error(\"<Class>: ...\")` raises grew {allowed} -> {count} "
                f"-- the Julia-layer taxonomy ratchet only shrinks (Issue #11146; "
                f"baseline docs/vm/EXCEPTION_TAXONOMY_JULIA_BASELINE.tsv)"
            )

if failures:
    print("FAIL: exception-taxonomy funnel violations (Issue #11146)\n")
    for f in failures:
        print("  - " + f)
    print(
        "\n%d violation(s). The funnel is subset_julia_vm_bytecode/src/error.rs:"
        "\n  VmError::exception_class() -- one exhaustive variant -> Julia class map."
        % len(failures)
    )
    sys.exit(1)

julia_residual = sum(observed.values()) if os.path.isdir(JULIA_ROOT) else 0
print(
    "OK: exception taxonomy funnel intact "
    "(%d VmError variants classified, %d Julia exception classes; "
    "no message/variant class contradiction, no hard-coded struct names, "
    "no catch-all arm; Julia-layer error(\"<Class>: ...\") residual %d, "
    "ratcheted)" % (len(variant_class), len(class_names), julia_residual)
)
PY
