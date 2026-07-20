#!/usr/bin/env bash
# check_compile_expr_local_shadow_guard.sh — bare-name fast paths must respect
# local/keyword shadowing (Issue #10044, bug #10034).
#
# Root cause guarded against: the compiler's identifier resolution
# (`Expr::Var(name, _)` arm in subset_julia_vm_compile/src/compile/expr/mod.rs) has
# special-cases that compile a bare name (`stdout`, `devnull`, `ARGS`, …)
# straight to a dedicated instruction. #10034 happened because such a
# special-case ran BEFORE checking whether the same name was already a
# keyword/local binding: `redirect_stdio(; stderr=...)` compiled the keyword
# parameter `stderr` as `PushStderr`, silently ignoring the local value.
#
# Invariant: every `name == "..."` special-case inside the `Expr::Var` arm must
# prove local/keyword bindings shadow it first, i.e. it must be
#   * inside (or on the same line as) a guard testing
#     `!self.locals.contains_key(name)` / `contains_key(name.as_str())` or
#     `!self.initialized_locals.contains(name)` / `contains(name.as_str())`, or
#   * explicitly annotated with a `// no-local-shadow: <reason>` comment on the
#     same line or within the 3 preceding lines, for names that can never be
#     introduced as a local/keyword binding.
#
# Note: `!self.locals.contains_key(name)` alone does NOT cover keyword
# parameters (that was exactly #10034); names reachable from keyword bindings
# must also be under `!self.initialized_locals.contains(name)`. This audit
# accepts either guard structurally — review new shadowable names for the
# stricter guard (see docs/vm/CHECKLISTS.md, "Compile-Time Bare-Name Fast
# Path Checklist").
#
# Usage (from the repository root):
#   bash scripts/check_compile_expr_local_shadow_guard.sh
#
# Exit code: 0 = every bare-name special-case is guarded or annotated,
#            1 = violation (or the audit's anchor disappeared — fail loud).
#
# Dependencies: python3 (stdlib only), bash 3.2+.

set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

TARGET="subset_julia_vm_compile/src/compile/expr/mod.rs"

if [[ ! -f "$TARGET" ]]; then
    echo "ERROR: $TARGET not found. Run from the repository root; if the file moved, update this audit (Issue #10044)." >&2
    exit 1
fi

python3 - "$TARGET" <<'PY'
import re
import sys

path = sys.argv[1]
src = open(path, encoding="utf-8").read()

# ---------------------------------------------------------------------------
# Mask string/char literals and comments so brace tracking is reliable.
# The file is rustfmt-formatted Rust; raw strings are not expected here.
# ---------------------------------------------------------------------------
masked = list(src)
i, n = 0, len(src)
NORMAL, LINE_COMMENT, BLOCK_COMMENT, STRING, CHAR = range(5)
state = NORMAL
while i < n:
    c = src[i]
    if state == NORMAL:
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            state = LINE_COMMENT
            masked[i] = masked[i + 1] = " "
            i += 2
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            state = BLOCK_COMMENT
            masked[i] = masked[i + 1] = " "
            i += 2
            continue
        if c == '"':
            state = STRING
            i += 1
            continue
        if c == "'":
            # Distinguish char literal from lifetime: 'x' or '\x...'
            if i + 1 < n and src[i + 1] == "\\":
                state = CHAR
                i += 1
                continue
            if i + 2 < n and src[i + 2] == "'":
                state = CHAR
                i += 1
                continue
            # lifetime — stays NORMAL
        i += 1
        continue
    if state == LINE_COMMENT:
        if c == "\n":
            state = NORMAL
        else:
            masked[i] = " "
        i += 1
        continue
    if state == BLOCK_COMMENT:
        if c == "*" and i + 1 < n and src[i + 1] == "/":
            masked[i] = masked[i + 1] = " "
            state = NORMAL
            i += 2
            continue
        if c != "\n":
            masked[i] = " "
        i += 1
        continue
    if state == STRING:
        if c == "\\" and i + 1 < n:
            masked[i] = " "
            if src[i + 1] != "\n":
                masked[i + 1] = " "
            i += 2
            continue
        if c == '"':
            state = NORMAL
        elif c != "\n":
            masked[i] = " "
        i += 1
        continue
    if state == CHAR:
        if c == "'":
            state = NORMAL
        elif c != "\n":
            masked[i] = " "
        i += 1
        continue
masked = "".join(masked)


def match_brace(start):
    """Index of the `}` matching the `{` at masked[start], or -1."""
    depth = 0
    for j in range(start, len(masked)):
        ch = masked[j]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return j
    return -1


# ---------------------------------------------------------------------------
# Locate the `Expr::Var(name, _) => {` arm (fail LOUD if the anchor moved —
# an audit that stops finding its target must not keep reporting OK).
# ---------------------------------------------------------------------------
anchor = re.search(r"Expr::Var\(name,[^)]*\)\s*=>", src)
if anchor is None:
    print(
        "ERROR: anchor `Expr::Var(name, _) =>` not found in "
        f"{path} — the identifier-resolution arm moved or was renamed. "
        "Update scripts/check_compile_expr_local_shadow_guard.sh so the "
        "local-shadow audit keeps guarding it (Issue #10044).",
        file=sys.stderr,
    )
    sys.exit(1)
arm_open = masked.find("{", anchor.end())
arm_close = match_brace(arm_open)
if arm_open == -1 or arm_close == -1:
    print(
        f"ERROR: could not delimit the Expr::Var arm body in {path} "
        "(unbalanced braces after the anchor). Fix the audit before "
        "trusting it (Issue #10044).",
        file=sys.stderr,
    )
    sys.exit(1)

NAME_ARG = r"name(?:\s*\.\s*as_str\s*\(\s*\))?"
GUARD_RE = re.compile(
    r"!\s*self\s*\.\s*(?:locals\s*\.\s*contains_key|initialized_locals\s*\.\s*contains)"
    r"\s*\(\s*" + NAME_ARG + r"\s*\)"
)
ANNOTATION = "no-local-shadow:"

# The audit owns a small Rust-source grammar. Keep its intentionally accepted
# projections explicit and independent of the production file (Issue #11604).
ACCEPTED_GUARD_FORMS = (
    ("direct local name", "!self.locals.contains_key(name)"),
    ("direct initialized-local name", "!self.initialized_locals.contains(name)"),
    ("interned-str projection", "!self.locals.contains_key(name.as_str())"),
    (
        "interned-str projection with whitespace",
        "! self . initialized_locals . contains ( name . as_str ( ) )",
    ),
)
REJECTED_GUARD_FORMS = (
    ("unguarded bare-name comparison", 'name == "stdout"'),
    ("non-negated local lookup", "self.locals.contains_key(name)"),
    ("unrelated projected name", "!self.locals.contains_key(other.as_str())"),
)

for label, form in ACCEPTED_GUARD_FORMS:
    if GUARD_RE.fullmatch(form) is None:
        print(
            f"ERROR: guard grammar conformance rejected accepted form '{label}': {form} "
            "(Issue #11604). Update NAME_ARG/GUARD_RE with the source carrier migration.",
            file=sys.stderr,
        )
        sys.exit(1)

for label, form in REJECTED_GUARD_FORMS:
    if GUARD_RE.search(form) is not None:
        print(
            f"ERROR: guard grammar conformance accepted forbidden form '{label}': {form} "
            "(Issue #11604). Keep unguarded and unrelated names outside the guard grammar.",
            file=sys.stderr,
        )
        sys.exit(1)

# Guard regions: for each guard condition inside the arm, the body of its
# `if` block (from the first `{` after the guard match to the matching `}`).
guard_regions = []
for g in GUARD_RE.finditer(src, anchor.start(), arm_close):
    body_open = masked.find("{", g.end(), arm_close + 1)
    if body_open == -1:
        continue
    body_close = match_brace(body_open)
    if body_close != -1:
        guard_regions.append((body_open, body_close))

line_starts = [0]
for k, ch in enumerate(src):
    if ch == "\n":
        line_starts.append(k + 1)


def line_no(pos):
    lo, hi = 0, len(line_starts) - 1
    while lo < hi:
        mid = (lo + hi + 1) // 2
        if line_starts[mid] <= pos:
            lo = mid
        else:
            hi = mid - 1
    return lo + 1  # 1-based


lines = src.splitlines()
violations = []
checked = 0
NAME_CMP_RE = re.compile(r'name\s*==\s*"')
for m in NAME_CMP_RE.finditer(src, anchor.start()):
    pos = m.start()
    if pos <= arm_open or pos >= arm_close:
        continue
    checked += 1
    ln = line_no(pos)
    line_text = lines[ln - 1]
    # (a) inline guard on the same line (e.g. `if name == "nothing" && !self.locals...`)
    if GUARD_RE.search(line_text):
        continue
    # (b) inside a guard block
    if any(lo < pos < hi for lo, hi in guard_regions):
        continue
    # (c) explicit annotation on the same line or the 3 preceding lines
    context = lines[max(0, ln - 4) : ln]
    if any(ANNOTATION in c for c in context):
        continue
    violations.append((ln, line_text.strip()))

if checked == 0:
    print(
        f"ERROR: found 0 `name == \"...\"` special-cases inside the Expr::Var arm of {path}. "
        "Either they all moved (update this audit) or the parse is broken — "
        "a zero-match audit guards nothing (Issue #10044).",
        file=sys.stderr,
    )
    sys.exit(1)

if violations:
    print(
        f"FAIL: {len(violations)} bare-name special-case(s) in the Expr::Var arm of {path} "
        "without a local-shadow guard (Issue #10044, bug #10034):",
        file=sys.stderr,
    )
    for ln, text in violations:
        print(f"  {path}:{ln}: {text}", file=sys.stderr)
    print(
        "Every compile-time bare-name fast path must prove local/keyword bindings shadow it:\n"
        "  * wrap it in `if !self.locals.contains_key(name)` (plus\n"
        "    `!self.initialized_locals.contains(name)` when the name is reachable from a\n"
        "    keyword binding — that was exactly bug #10034), or\n"
        "  * annotate it with `// no-local-shadow: <reason>` if the name can never be a\n"
        "    local/keyword binding.\n"
        "See docs/vm/CHECKLISTS.md \"Compile-Time Bare-Name Fast Path Checklist\".",
        file=sys.stderr,
    )
    sys.exit(1)

print(
    f"OK: all {checked} bare-name special-cases in the Expr::Var arm are local-shadow "
    "guarded or explicitly annotated (Issue #10044)."
)
PY
