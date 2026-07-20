#!/usr/bin/env bash
# check_unregistered_fixtures.sh
#
# Fail if any subset_julia_vm/tests/fixtures/**/*.jl is NOT covered, where
# "covered" means one of:
#   (a) registered by a `[[tests]]` entry in a category manifest.toml, OR
#   (b) referenced by an `include("...")` / `evalfile("...")` from another
#       fixture (auto-detected — intentional helper/data files), OR
#   (c) listed in docs/vm/FIXTURE_COVERAGE_ALLOWLIST.tsv (with a reason).
#
# The fixture harness silently skips any .jl with no manifest entry, so an
# unregistered fixture never runs — it can be red on the CLI while the full
# suite stays green (Issue #9360). This audit closes that gap.
#
# It also flags STALE allowlist rows (a listed path that is now registered or no
# longer exists) so the allowlist shrinks over time.
#
# Usage (from repo root): bash scripts/check_unregistered_fixtures.sh
# Test-only sandbox overrides:
#   SJULIA_FIXTURE_COVERAGE_FIXTURES_DIR=/tmp/fixtures
#   SJULIA_FIXTURE_COVERAGE_ALLOWLIST=/tmp/allowlist.tsv
# Exit code: 0 = all fixtures covered, 1 = uncovered fixtures or stale rows.
#
# The set logic runs in an inline python3 heredoc (shellcheck treats it as data);
# the bash wrapper stays trivially bash-3.2 compatible.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

FIXTURES_DIR="${SJULIA_FIXTURE_COVERAGE_FIXTURES_DIR:-subset_julia_vm/tests/fixtures}"
ALLOWLIST="${SJULIA_FIXTURE_COVERAGE_ALLOWLIST:-docs/vm/FIXTURE_COVERAGE_ALLOWLIST.tsv}"

if [[ ! -d "$FIXTURES_DIR" ]]; then
    echo "ERROR: fixtures directory not found: $FIXTURES_DIR"
    echo "Run this script from the repository root."
    exit 1
fi

FIXTURES_DIR="$FIXTURES_DIR" ALLOWLIST="$ALLOWLIST" python3 - <<'PY'
import os
import re
import sys
import unicodedata

fixtures_dir = os.environ["FIXTURES_DIR"]
allowlist_path = os.environ["ALLOWLIST"]

file_re = re.compile(r'^\s*file\s*=\s*"([^"]+)"')
literal_call_re = re.compile(r'(?:include|evalfile)\(\s*"([^"\\]*)"\s*\)')

# Prefer a real TOML parse (Python >= 3.11) so an entry sitting under an
# unknown-key typo (e.g. `[[test]]` instead of `[[tests]]`) is NOT mistaken
# for a registration (Issue #9486). Degrade VISIBLY: the textual fallback
# prints a NOTE so a weaker local signal is never silent.
try:
    import tomllib
except ImportError:  # Python < 3.11 (e.g. macOS system python3)
    tomllib = None
    print(
        "NOTE: python tomllib unavailable (python < 3.11) — falling back to "
        'textual `file = "..."` line scanning. This weaker mode counts entries '
        "under an unknown-key typo (e.g. [[test]]) as registered; the Rust "
        "manifest parse (deny_unknown_fields) remains the loud gate "
        "(Issue #9486).",
        file=sys.stderr,
    )


def iter_registered_files(manifest):
    """Yield the `file` value of each [[tests]] entry in `manifest`."""
    if tomllib is not None:
        with open(manifest, "rb") as fh:
            data = tomllib.load(fh)
        for t in data.get("tests", []):
            fp = t.get("file")
            if isinstance(fp, str):
                yield fp
        return
    with open(manifest, encoding="utf-8") as fh:
        for line in fh:
            mo = file_re.match(line)
            if mo:
                yield mo.group(1)


def norm(p):
    return os.path.normpath(p)


def is_identifier_continuation(char):
    if not char:
        return False
    category = unicodedata.category(char)
    return (
        char in "_!?′″‴⁗"
        or category[0] in "LN"
        # Julia permits Me and selected Sm codepoints too. Treating every Sm as
        # a continuation is deliberately conservative at this audit boundary:
        # a false negative asks for an explicit helper row, while a false
        # positive could silently hide an uncovered fixture.
        or category in {"Mn", "Mc", "Me", "Pc", "Sk", "Sc", "Sm", "So"}
    )


def skip_delimited(text, start, delimiter):
    """Return the first offset after a quoted Julia string/command literal."""
    index = start + len(delimiter)
    while index < len(text):
        if text.startswith(delimiter, index):
            return index + len(delimiter)
        if text[index] == "\\":
            index += 2
        else:
            index += 1
    return len(text)


def skip_char_literal(text, start):
    """Skip a Julia character literal, but not the postfix-adjoint apostrophe."""
    index = start + 1
    if index >= len(text) or text[index] == "\n":
        return start + 1
    if text[index] == "\\":
        index += 1
        while index < len(text) and text[index] != "\n" and text[index] != "'":
            index += 1
    else:
        index += 1
    return index + 1 if index < len(text) and text[index] == "'" else start + 1


def skip_block_comment(text, start):
    index = start + 2
    depth = 1
    while index < len(text) and depth:
        if text.startswith("#=", index):
            depth += 1
            index += 2
        elif text.startswith("=#", index):
            depth -= 1
            index += 2
        else:
            index += 1
    return index


def find_matching_paren(text, open_index):
    """Find a closing paren while ignoring Julia comments and literals."""
    index = open_index + 1
    depth = 1
    while index < len(text):
        if text.startswith("#=", index):
            index = skip_block_comment(text, index)
        elif text[index] == "#":
            newline = text.find("\n", index)
            index = len(text) if newline == -1 else newline + 1
        elif text.startswith('"""', index):
            index = skip_delimited(text, index, '"""')
        elif text[index] == '"':
            index = skip_delimited(text, index, '"')
        elif text[index] == "'":
            index = skip_char_literal(text, index)
        elif text[index] == "`":
            index = skip_delimited(text, index, "`")
        elif text[index] == "(":
            depth += 1
            index += 1
        elif text[index] == ")":
            depth -= 1
            if depth == 0:
                return index
            index += 1
        else:
            index += 1
    return len(text)


def scan_string_literal(text, start, delimiter):
    """Skip a string and collect calls executed in `$(...)` interpolation."""
    targets = []
    index = start + len(delimiter)
    while index < len(text):
        if text.startswith(delimiter, index):
            return index + len(delimiter), targets
        if text[index] == "\\":
            index += 2
            continue
        if text.startswith("$(", index):
            close = find_matching_paren(text, index + 1)
            targets.extend(iter_literal_include_targets(text[index + 2 : close]))
            index = min(close + 1, len(text))
            continue
        index += 1
    return len(text), targets


def skip_quote_block(text, start):
    """Skip a `quote ... end` expression, including nested Julia blocks."""
    openers = {
        "begin",
        "quote",
        "if",
        "for",
        "while",
        "try",
        "function",
        "macro",
        "let",
        "struct",
        "module",
        "baremodule",
        "do",
    }
    index = start
    depth = 1
    while index < len(text):
        if text.startswith("#=", index):
            index = skip_block_comment(text, index)
            continue
        if text[index] == "#":
            newline = text.find("\n", index)
            index = len(text) if newline == -1 else newline + 1
            continue
        if text.startswith('"""', index):
            index = skip_delimited(text, index, '"""')
            continue
        if text[index] == '"':
            index = skip_delimited(text, index, '"')
            continue
        if text[index] == "'":
            index = skip_char_literal(text, index)
            continue
        if text[index] == "`":
            index = skip_delimited(text, index, "`")
            continue
        if is_identifier_continuation(text[index]):
            end = index + 1
            while end < len(text) and is_identifier_continuation(text[end]):
                end += 1
            token = text[index:end]
            if token in openers:
                depth += 1
            elif token == "end":
                depth -= 1
                if depth == 0:
                    return end
            index = end
            continue
        index += 1
    return len(text)


def iter_literal_include_targets(text):
    """Yield literal include/evalfile targets from executable Julia source."""
    index = 0
    while index < len(text):
        if text.startswith("#=", index):
            index = skip_block_comment(text, index)
            continue
        if text[index] == "#":
            newline = text.find("\n", index)
            index = len(text) if newline == -1 else newline + 1
            continue
        if text.startswith('"""', index):
            previous = text[index - 1] if index else ""
            if is_identifier_continuation(previous):
                index = skip_delimited(text, index, '"""')
            else:
                index, targets = scan_string_literal(text, index, '"""')
                yield from targets
            continue
        if text[index] == '"':
            previous = text[index - 1] if index else ""
            if is_identifier_continuation(previous):
                index = skip_delimited(text, index, '"')
            else:
                index, targets = scan_string_literal(text, index, '"')
                yield from targets
            continue
        if text[index] == "'":
            index = skip_char_literal(text, index)
            continue
        if text[index] == "`":
            index = skip_delimited(text, index, "`")
            continue
        if text.startswith(":(", index):
            close = find_matching_paren(text, index + 1)
            index = min(close + 1, len(text))
            continue
        if text.startswith("quote", index):
            previous = text[index - 1] if index else ""
            following = text[index + len("quote")] if index + len("quote") < len(text) else ""
            if not is_identifier_continuation(previous) and not is_identifier_continuation(following):
                index = skip_quote_block(text, index + len("quote"))
                continue

        match = literal_call_re.match(text, index)
        if match:
            previous = text[index - 1] if index else ""
            if not is_identifier_continuation(previous) and (
                not previous or previous not in ":@"
            ):
                yield match.group(1)
                index = match.end()
                continue
        index += 1


# 1. All .jl files under the fixtures tree.
all_jl = set()
for root, _dirs, files in os.walk(fixtures_dir):
    for name in files:
        if name.endswith(".jl"):
            all_jl.add(norm(os.path.join(root, name)))

# 2. Registered files: every `file = "..."` in a category / root manifest,
#    applying the same category-prefix rule as build.rs.
registered = set()
manifest_paths = [os.path.join(fixtures_dir, "manifest.toml")]
for entry in sorted(os.listdir(fixtures_dir)):
    cand = os.path.join(fixtures_dir, entry, "manifest.toml")
    if os.path.isfile(cand):
        manifest_paths.append(cand)

for manifest in manifest_paths:
    if not os.path.isfile(manifest):
        continue
    category = os.path.basename(os.path.dirname(manifest))
    is_root = os.path.dirname(manifest) == fixtures_dir
    for fp in iter_registered_files(manifest):
        if "/" not in fp and not is_root:
            fp = f"{category}/{fp}"
        registered.add(norm(os.path.join(fixtures_dir, fp)))

# 3. include()/evalfile() targets from any fixture are intentional helpers/data.
#    Resolve against (a) the including file's directory and (b) the crate root
#    (subset_julia_vm), since evalfile paths are written relative to the harness
#    CWD.
crate_root = os.path.dirname(os.path.dirname(fixtures_dir))  # subset_julia_vm
referenced = set()
for jl in all_jl:
    d = os.path.dirname(jl)
    try:
        text = open(jl, encoding="utf-8", errors="replace").read()
    except OSError:
        continue
    for target in iter_literal_include_targets(text):
        for base in (d, crate_root, "."):
            resolved = norm(os.path.join(base, target))
            if resolved in all_jl:
                referenced.add(resolved)

# 4. Allowlist rows (path<TAB>reason).
allowlist = {}
allowlist_errors = []
if os.path.isfile(allowlist_path):
    with open(allowlist_path, encoding="utf-8") as fh:
        for lineno, raw in enumerate(fh, 1):
            line = raw.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            rel = parts[0].strip()
            reason = parts[1].strip() if len(parts) > 1 else ""
            path = norm(os.path.join(fixtures_dir, rel))
            if path in allowlist:
                first_lineno = allowlist[path][2]
                allowlist_errors.append(
                    f"  {allowlist_path}:{lineno}: {rel} — duplicate allowlist path "
                    f"(first listed at line {first_lineno})"
                )
                continue
            allowlist[path] = (rel, reason, lineno)

covered = registered | referenced

# 5a. Uncovered fixtures = a real gap.
uncovered = sorted(all_jl - covered - set(allowlist))

# 5b. Stale allowlist rows (now covered, or non-existent file, or missing reason).
stale = []
stale.extend(allowlist_errors)
for path, (rel, reason, lineno) in sorted(allowlist.items()):
    if path not in all_jl:
        stale.append(f"  {allowlist_path}:{lineno}: {rel} — file does not exist")
    elif path in covered:
        stale.append(
            f"  {allowlist_path}:{lineno}: {rel} — now registered/referenced; remove this row"
        )
    elif not reason:
        stale.append(f"  {allowlist_path}:{lineno}: {rel} — allowlist row is missing a reason")

failed = False
if uncovered:
    failed = True
    print(
        f"ERROR: {len(uncovered)} fixture .jl file(s) are neither registered in a "
        "manifest.toml, referenced via include()/evalfile(), nor allowlisted "
        "(Issue #9360):"
    )
    for p in uncovered:
        rel = os.path.relpath(p, fixtures_dir)
        print(f"  {rel}")
    print("")
    print("Fix: register it with a [[tests]] entry (verify `expected` by running it")
    print("under sjulia AND upstream julia), or add it to")
    print(f"{allowlist_path} with a reason if it is an intentional non-test helper.")

if stale:
    failed = True
    print(f"ERROR: {len(stale)} stale row(s) in {allowlist_path}:")
    for s in stale:
        print(s)

if failed:
    sys.exit(1)

print(
    f"OK: all {len(all_jl)} fixture .jl files are covered "
    f"({len(registered)} registered, {len(referenced)} include/evalfile helpers, "
    f"{len(allowlist)} allowlisted) (Issue #9360)."
)
PY
