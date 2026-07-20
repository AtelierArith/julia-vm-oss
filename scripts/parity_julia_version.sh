#!/usr/bin/env bash
# parity_julia_version.sh
#
# Resolve WHICH julia command parity-verification tooling must run, by
# checking the julia on PATH against the parity target series in the
# repository-root PARITY_TARGET file (single source — see
# docs/vm/PARITY_TARGET.md, Issue #8644 / #8667).
#
# Behavior:
#   - PATH julia matches the target series (MAJOR.MINOR)  → print "julia".
#   - PATH julia mismatches but juliaup can provide the series
#     (`julia +X.Y` works)                                 → print "julia +X.Y"
#     and warn on stderr.
#   - Mismatch and no juliaup channel: warn on stderr; without --strict
#     still print "julia" (parity runs proceed, loudly); with --strict
#     exit non-zero so no parity verification silently runs against the
#     wrong Julia.
#
# Usage (from a parity script):
#   JULIA_CMD=$(bash scripts/parity_julia_version.sh)          # warn only
#   JULIA_CMD=$(bash scripts/parity_julia_version.sh --strict) # fail on drift
#   $JULIA_CMD --startup-file=no file.jl   # NOTE: unquoted — may be 2 words
#
# Also honors SJULIA_PARITY_STRICT=1 as an env-var equivalent of --strict.
#
# NAMING: deliberately NOT named `check_*.sh` — this is a resolver helper
# used by parity scripts, not a standalone CI audit gate.

set -euo pipefail

strict=0
if [[ "${1:-}" == "--strict" ]]; then
    strict=1
elif [[ "${SJULIA_PARITY_STRICT:-0}" == "1" ]]; then
    strict=1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target_file="$repo_root/PARITY_TARGET"

if [[ ! -f "$target_file" ]]; then
    echo "ERROR: PARITY_TARGET file not found at $target_file" >&2
    echo "       (see docs/vm/PARITY_TARGET.md, Issue #8644)" >&2
    exit 2
fi

# First non-comment, non-empty line is the target series MAJOR.MINOR.
target_series="$(grep -v '^[[:space:]]*#' "$target_file" | grep -v '^[[:space:]]*$' | head -1 | tr -d '[:space:]')"

if [[ ! "$target_series" =~ ^[0-9]+\.[0-9]+$ ]]; then
    echo "ERROR: PARITY_TARGET does not contain a MAJOR.MINOR series (got: '$target_series')" >&2
    exit 2
fi

if ! command -v julia >/dev/null 2>&1; then
    echo "ERROR: 'julia' is not on PATH. Install the Julia $target_series.x" >&2
    echo "       series (see docs/vm/PARITY_TARGET.md)." >&2
    exit 2
fi

# `julia --version` prints e.g. "julia version 1.12.6".
path_version="$(julia --version 2>/dev/null | sed -n 's/^julia version \([0-9][0-9.]*\).*/\1/p')"
path_series="$(printf '%s' "$path_version" | cut -d. -f1-2)"

if [[ "$path_series" == "$target_series" ]]; then
    echo "julia"
    exit 0
fi

# juliaup environments: `julia +X.Y` selects a channel. Verify the channel
# actually resolves to the target series before recommending it.
channel_version="$(julia "+$target_series" --version 2>/dev/null | sed -n 's/^julia version \([0-9][0-9.]*\).*/\1/p' || true)"
channel_series="$(printf '%s' "$channel_version" | cut -d. -f1-2)"
if [[ -n "$channel_version" && "$channel_series" == "$target_series" ]]; then
    echo "WARNING: julia on PATH is $path_version but the parity target series" >&2
    echo "         is $target_series (PARITY_TARGET; docs/vm/PARITY_TARGET.md)." >&2
    echo "         Auto-selecting juliaup channel: julia +$target_series ($channel_version)." >&2
    echo "julia +$target_series"
    exit 0
fi

echo "WARNING: julia on PATH is $path_version but the parity target series is" >&2
echo "         $target_series (PARITY_TARGET; docs/vm/PARITY_TARGET.md), and no" >&2
echo "         juliaup channel '+$target_series' is available." >&2
if [[ "$strict" == "1" ]]; then
    echo "ERROR: refusing to run parity verification against a non-target Julia (--strict)." >&2
    exit 1
fi
echo "         Proceeding with the PATH julia; version-dependent mismatches" >&2
echo "         may be Julia-version drift, not sjulia bugs." >&2
echo "julia"
