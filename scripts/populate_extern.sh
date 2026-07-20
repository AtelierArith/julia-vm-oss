#!/usr/bin/env bash
# populate_extern.sh — clone or update extern/ reference packages from MANIFEST.tsv
#
# Usage:
#   bash scripts/populate_extern.sh              # all packages in MANIFEST
#   bash scripts/populate_extern.sh Rotations.jl # single package by name
#
# Purpose (Issue #9000):
#   extern/ contains Julia package source trees used as parity oracles for
#   fixture implementation and reference for bundled-package ports. The
#   directory is .gitignore'd (too large to track), so this script provides
#   reproducible re-population from the pinned versions in extern/MANIFEST.tsv.
#
#   After running, verify each entry with:
#     git -C extern/<Pkg>.jl rev-parse HEAD
#   and update the commit_sha column in extern/MANIFEST.tsv.
#
# Exits non-zero if any clone/checkout fails.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MANIFEST="$REPO_ROOT/extern/MANIFEST.tsv"
EXTERN_DIR="$REPO_ROOT/extern"

if [[ ! -f "$MANIFEST" ]]; then
  echo "ERROR: extern/MANIFEST.tsv not found at $MANIFEST" >&2
  exit 1
fi

# Parse the filter argument (optional)
FILTER="${1:-}"

mkdir -p "$EXTERN_DIR"

FAILED=()

# Read MANIFEST.tsv (skip comment lines and blank lines; tab-separated)
while IFS=$'\t' read -r name version upstream_url commit_sha fetch_date notes; do
  # Skip comment lines and the header line
  [[ "$name" =~ ^#.*$ ]] && continue
  [[ "$name" == "name" ]] && continue
  [[ -z "$name" ]] && continue

  # Apply filter if specified
  if [[ -n "$FILTER" && "$name" != "$FILTER" ]]; then
    continue
  fi

  target="$EXTERN_DIR/$name"
  tag="$version"

  echo "==> $name ($tag) from $upstream_url"

  if [[ -d "$target/.git" ]]; then
    echo "    Updating existing clone…"
    git -C "$target" fetch --quiet origin
    if ! git -C "$target" checkout --quiet "$tag" 2>/dev/null; then
      echo "    WARNING: tag $tag not found; trying to fetch tags…"
      git -C "$target" fetch --quiet --tags origin
      if ! git -C "$target" checkout --quiet "$tag" 2>/dev/null; then
        echo "    ERROR: cannot checkout $tag in $target" >&2
        FAILED+=("$name")
        continue
      fi
    fi
  else
    echo "    Cloning (depth=1 at $tag)…"
    # Try shallow clone at the tag first; fall back to full clone if the tag
    # is not directly fetchable (some repos use annotated tags that require
    # --no-single-branch).
    if ! git clone --quiet --depth 1 --branch "$tag" "$upstream_url" "$target" 2>/dev/null; then
      echo "    Shallow clone failed; falling back to full clone…"
      if ! git clone --quiet "$upstream_url" "$target"; then
        echo "    ERROR: cannot clone $upstream_url" >&2
        FAILED+=("$name")
        continue
      fi
      if ! git -C "$target" checkout --quiet "$tag" 2>/dev/null; then
        git -C "$target" fetch --quiet --tags origin
        if ! git -C "$target" checkout --quiet "$tag"; then
          echo "    ERROR: cannot checkout $tag in $target" >&2
          FAILED+=("$name")
          continue
        fi
      fi
    fi
  fi

  actual_sha="$(git -C "$target" rev-parse HEAD)"
  echo "    OK: $name @ $tag = $actual_sha"

  # Remind the operator to update MANIFEST if the SHA is still UNVERIFIED.
  if [[ "$commit_sha" == "UNVERIFIED" ]]; then
    echo "    NOTE: commit_sha in MANIFEST.tsv is UNVERIFIED — update it to: $actual_sha"
  elif [[ "$commit_sha" != "$actual_sha" ]]; then
    echo "    WARNING: MANIFEST records $commit_sha but checked out $actual_sha"
    echo "    If the upstream tag moved, update MANIFEST.tsv."
  fi

done < "$MANIFEST"

if [[ ${#FAILED[@]} -gt 0 ]]; then
  echo ""
  echo "FAILED packages:" >&2
  for f in "${FAILED[@]}"; do echo "  $f" >&2; done
  exit 1
fi

echo ""
echo "Done. All extern/ packages populated."
echo ""
echo "Next steps if any SHA was UNVERIFIED:"
echo "  1. Run: git -C extern/<Pkg>.jl rev-parse HEAD"
echo "  2. Update the commit_sha column in extern/MANIFEST.tsv"
echo "  3. Commit the updated MANIFEST.tsv"
