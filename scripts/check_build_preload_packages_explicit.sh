#!/usr/bin/env bash
# Audit build.sh's iOS preload-package cache handoff (Issue #10160).
#
# The preload cache is layout-sensitive: a cache generated from the union of all
# sample imports does not match real samples' exact package closure/order, so it
# becomes a dead embedded artifact.  build.sh must therefore keep preload
# packages empty by default and only generate/embed the cache when
# SJULIA_PRELOAD_PACKAGES is explicitly provided.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_SH="$ROOT_DIR/build.sh"

python3 - "$BUILD_SH" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
src = path.read_text(encoding="utf-8")
errors = []

if 'PRELOAD_PACKAGES_FOR_BUILD="$(detect_sample_preload_packages)"' in src:
    errors.append(
        "build.sh still auto-detects a union of iOS sample imports when "
        "SJULIA_PRELOAD_PACKAGES is unset"
    )

if "detected from iOS samples" in src:
    errors.append(
        "build.sh still advertises sample-based preload package detection; "
        "the default must be disabled/empty"
    )

if 'PRELOAD_PACKAGES_FOR_BUILD=""' not in src:
    errors.append(
        "build.sh should assign PRELOAD_PACKAGES_FOR_BUILD=\"\" for the "
        "unset SJULIA_PRELOAD_PACKAGES default"
    )

precompile = 'run_with_heartbeat "Preload cache generation"'
if precompile in src:
    idx = src.index(precompile)
    context = src[max(0, idx - 350):idx]
    if '[[ -n "$PRELOAD_PACKAGES_FOR_BUILD" ]]' not in context:
        errors.append(
            "preload cache generation must be guarded by a non-empty explicit "
            "PRELOAD_PACKAGES_FOR_BUILD"
        )
else:
    errors.append("could not find preload cache generation command in build.sh")

embed = 'export SJULIA_PRELOAD_CACHE="$PRELOAD_CACHE"'
if embed in src:
    idx = src.index(embed)
    context = src[max(0, idx - 350):idx]
    if '[[ -n "$PRELOAD_PACKAGES_FOR_BUILD" ]]' not in context:
        errors.append(
            "SJULIA_PRELOAD_CACHE must only be exported when explicit preload "
            "packages are non-empty"
        )
else:
    errors.append("could not find SJULIA_PRELOAD_CACHE export in build.sh")

if errors:
    for error in errors:
        print(f"ERROR: {error}")
    raise SystemExit(1)

print("OK: build.sh preload packages are explicit-only and default-empty")
PY
