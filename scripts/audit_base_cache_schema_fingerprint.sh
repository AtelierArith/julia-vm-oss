#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
crate_dir="$repo_root/subset_julia_vm"
manifest="$crate_dir/src/compile/base_cache_schema_files.txt"
snapshot="$crate_dir/src/compile/base_cache_schema_fingerprint.txt"
precompile_rs="$crate_dir/src/compile/precompile.rs"

if [[ ! -f "$manifest" ]]; then
  echo "ERROR: missing Base cache schema manifest: $manifest" >&2
  exit 1
fi

if [[ ! -f "$snapshot" ]]; then
  echo "ERROR: missing Base cache schema snapshot: $snapshot" >&2
  exit 1
fi

current_version="$(
  sed -n 's/^const CACHE_VERSION: u32 = \([0-9][0-9]*\);$/\1/p' "$precompile_rs"
)"
if [[ -z "$current_version" ]]; then
  echo "ERROR: could not read CACHE_VERSION from $precompile_rs" >&2
  exit 1
fi

snapshot_version="$(sed -n 's/^CACHE_VERSION=\([0-9][0-9]*\)$/\1/p' "$snapshot")"
snapshot_fingerprint="$(sed -n 's/^SCHEMA_FINGERPRINT=\([0-9a-f][0-9a-f]*\)$/\1/p' "$snapshot")"

if [[ -z "$snapshot_version" || -z "$snapshot_fingerprint" ]]; then
  echo "ERROR: snapshot must contain CACHE_VERSION=<n> and SCHEMA_FINGERPRINT=<sha256>" >&2
  exit 1
fi

tmp="$(mktemp)"
cleanup() {
  rm -f "$tmp"
}
trap cleanup EXIT

while IFS= read -r raw_line || [[ -n "$raw_line" ]]; do
  line="${raw_line%%#*}"
  line="${line#"${line%%[![:space:]]*}"}"
  line="${line%"${line##*[![:space:]]}"}"
  if [[ -z "$line" ]]; then
    continue
  fi
  if [[ "$line" = /* ]]; then
    echo "ERROR: schema manifest paths must be relative: $line" >&2
    exit 1
  fi
  path="$crate_dir/$line"
  if [[ ! -f "$path" ]]; then
    echo "ERROR: schema manifest references missing file: $line" >&2
    exit 1
  fi
  {
    printf '%s\0' "$line"
    cat "$path"
    printf '\0'
  } >> "$tmp"
done < "$manifest"

current_fingerprint="$(shasum -a 256 "$tmp" | awk '{print $1}')"

failed=0
if [[ "$current_version" != "$snapshot_version" ]]; then
  echo "ERROR: Base cache schema snapshot CACHE_VERSION is stale." >&2
  echo "  current:  $current_version" >&2
  echo "  snapshot: $snapshot_version" >&2
  failed=1
fi

if [[ "$current_fingerprint" != "$snapshot_fingerprint" ]]; then
  echo "ERROR: Base cache schema fingerprint changed." >&2
  echo "  current:  $current_fingerprint" >&2
  echo "  snapshot: $snapshot_fingerprint" >&2
  echo "Update CACHE_VERSION and $snapshot together when serialized Base cache schema inputs change." >&2
  failed=1
fi

if [[ "$failed" -ne 0 ]]; then
  exit 1
fi

echo "OK: Base cache schema fingerprint matches CACHE_VERSION $current_version"
