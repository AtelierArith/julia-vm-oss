#!/usr/bin/env bash
# check_fixture_manifests.sh
#
# Fail if any subset_julia_vm/tests/fixtures/*/manifest.toml (a) does not parse
# as TOML, or (b) registers ZERO `[[tests]]` entries.
#
# A malformed category manifest (e.g. a dropped `[[tests]]` header from a botched
# merge resolution) used to make the fixture harness silently register 0 tests
# for the whole category, and the full-suite gate stayed green with that
# category's coverage deleted (the 2026-07-06 bigfloat incident, repaired in
# PR #9359, tracked by Issue #9378). build.rs and the always-run
# `every_category_manifest_parses_and_registers_tests_9378` fixture test now also
# enforce this; this script is the fast pre-compile CI signal.
#
# Usage (from repo root): bash scripts/check_fixture_manifests.sh
# Exit code: 0 = all category manifests parse and register >= 1 test, else 1.
#
# The parse/count logic runs in an inline python3 heredoc (shellcheck treats it
# as data); the bash wrapper stays trivially bash-3.2 compatible.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

FIXTURES_DIR="subset_julia_vm/tests/fixtures"

if [[ ! -d "$FIXTURES_DIR" ]]; then
    echo "ERROR: fixtures directory not found: $FIXTURES_DIR"
    echo "Run this script from the repository root."
    exit 1
fi

FIXTURES_DIR="$FIXTURES_DIR" python3 - <<'PY'
import os
import re
import sys

fixtures_dir = os.environ["FIXTURES_DIR"]

# Prefer a real TOML parser when available (Python >= 3.11); otherwise fall back
# to a minimal `[[tests]]`-block scan that mirrors build.rs.
try:
    import tomllib  # type: ignore

    def count_tests(text):
        data = tomllib.loads(text)
        tests = data.get("tests", [])
        if not isinstance(tests, list):
            raise ValueError("`tests` is not an array")
        return len(tests)
except Exception:  # noqa: BLE001 - tomllib missing on older interpreters
    # Degrade VISIBLY (Issue #9486): the textual fallback is weaker than a
    # real TOML parse (it cannot detect unknown-key typos like [[test]]), so
    # a NOTE makes the reduced local signal explicit instead of silent.
    print(
        "NOTE: python tomllib unavailable (python < 3.11) — falling back to "
        "textual [[tests]]/file= line counting. This weaker mode cannot detect "
        "unknown-key typos (e.g. [[test]]); the Rust manifest parse "
        "(deny_unknown_fields) remains the loud gate (Issue #9486).",
        file=sys.stderr,
    )
    tests_hdr_re = re.compile(r'^\s*\[\[tests\]\]\s*$')
    file_re = re.compile(r'^\s*file\s*=\s*"[^"]+"')

    def count_tests(text):
        # Count [[tests]] blocks that actually carry a `file = "..."` key so a
        # dangling header alone does not count as a registered test.
        headers = 0
        files = 0
        for line in text.splitlines():
            if tests_hdr_re.match(line):
                headers += 1
            elif file_re.match(line):
                files += 1
        if headers == 0:
            return 0
        return min(headers, files)


problems = []
category_manifests = []
for entry in sorted(os.listdir(fixtures_dir)):
    cand = os.path.join(fixtures_dir, entry, "manifest.toml")
    if os.path.isfile(cand):
        category_manifests.append(cand)

if not category_manifests:
    print(f"ERROR: no category manifests found under {fixtures_dir}")
    sys.exit(1)

for manifest in category_manifests:
    try:
        text = open(manifest, encoding="utf-8").read()
    except OSError as exc:
        problems.append(f"  {manifest}: unreadable ({exc})")
        continue
    try:
        n = count_tests(text)
    except Exception as exc:  # noqa: BLE001
        problems.append(f"  {manifest}: FAILED TO PARSE ({exc})")
        continue
    if n == 0:
        problems.append(
            f"  {manifest}: parsed but registers 0 tests "
            "(a malformed/dropped [[tests]] header silently deletes the category)"
        )

if problems:
    print(
        f"ERROR: {len(problems)} category manifest(s) are malformed or register 0 "
        "tests (Issue #9378):"
    )
    for p in problems:
        print(p)
    sys.exit(1)

print(
    f"OK: all {len(category_manifests)} category manifests parse and register "
    ">= 1 test (Issue #9378)."
)
PY
