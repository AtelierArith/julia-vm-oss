#!/usr/bin/env bash
# audit_base_cache_schema_fingerprint.sh — Base cache schema fingerprint ratchet
# (Issues #8444/#9623/#10051/#10688).
#
# Guards against serialized-Base-cache schema changes (Instr/BuiltinId/
# Intrinsic/Value wire shapes, method-table wire structs, inference cache
# keys, ...) that land without a matching CACHE_VERSION bump and reviewable
# snapshot update.
#
# The script hashes the exact files listed in
# `subset_julia_vm_compile/src/compile/base_cache_schema_files.txt`, sorted with
# `LC_ALL=C sort` before hashing (Issue #10051 slice A) so the result no
# longer depends on the manifest's line order — reordering entries in the
# manifest (with no content change) used to silently change the audited
# "current" fingerprint; it can't anymore.
#
# The Rust schema-fingerprint guard independently checks that this audit's
# result for the current manifest matches the embedded
# `SJULIA_BASE_CACHE_SCHEMA_HASH`. The shell and build implementations retain
# separate sorting code, but ordering drift cannot remain green.
#
# NOTE: this is a CI/review-hygiene gate, not the runtime corruption guard.
# `deserialize_base_cache` independently recomputes `schema_fingerprint`,
# `enum_variant_fingerprint`, and `compiler_build_fingerprint` at load time
# and rejects any mismatching cache before payload decode regardless of
# whether this snapshot (or CACHE_VERSION) was updated — see
# `docs/vm/CACHE_ARCHITECTURE.md`. This script exists so a PR diff visibly
# shows "schema moved" instead of the fingerprint silently drifting unread,
# and so `CACHE_VERSION` (a human-meaningful changelog marker) is bumped in
# step.
#
# Normal check:
#   bash scripts/audit_base_cache_schema_fingerprint.sh
#
# Refresh the snapshot after a deliberate schema change (bump CACHE_VERSION
# in subset_julia_vm_compile/src/compile/precompile.rs first, then run):
#   bash scripts/audit_base_cache_schema_fingerprint.sh --update

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
crate_dir="$repo_root/subset_julia_vm_compile"
manifest="$crate_dir/src/compile/base_cache_schema_files.txt"
snapshot="$crate_dir/src/compile/base_cache_schema_fingerprint.txt"
precompile_rs="$crate_dir/src/compile/precompile.rs"
update_cmd="bash scripts/audit_base_cache_schema_fingerprint.sh --update"

usage() {
  echo "usage: $(basename "$0") [--update]" >&2
}

update=0
if [[ "${1:-}" == "--update" ]]; then
  update=1
  shift
fi

if [[ "$#" -ne 0 ]]; then
  usage
  exit 2
fi

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

# Pass 1: parse + validate every manifest entry (strip comments/whitespace,
# reject absolute paths / missing files) while preserving manifest order for
# error reporting.
paths_tmp="$(mktemp)"
cleanup() {
  rm -f "$paths_tmp" "$sorted_tmp" "$hash_input_tmp"
}
sorted_tmp="$(mktemp)"
hash_input_tmp="$(mktemp)"
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
  printf '%s\n' "$line" >> "$paths_tmp"
done < "$manifest"

# Pass 2: hash in the same order `build.rs` uses (`Vec<PathBuf>::sort()`), so
# the audited fingerprint matches the real embedded
# `SJULIA_BASE_CACHE_SCHEMA_HASH` regardless of manifest line order.
LC_ALL=C sort "$paths_tmp" > "$sorted_tmp"

while IFS= read -r line; do
  [[ -z "$line" ]] && continue
  path="$crate_dir/$line"
  {
    printf '%s\0' "$line"
    cat "$path"
    printf '\0'
  } >> "$hash_input_tmp"
done < "$sorted_tmp"

current_fingerprint="$(shasum -a 256 "$hash_input_tmp" | awk '{print $1}')"

if [[ "$update" -eq 1 ]]; then
  {
    printf 'CACHE_VERSION=%s\n' "$current_version"
    printf 'SCHEMA_FINGERPRINT=%s\n' "$current_fingerprint"
  } > "$snapshot"
  echo "Updated Base cache schema fingerprint snapshot:"
  echo "  CACHE_VERSION=$current_version"
  echo "  SCHEMA_FINGERPRINT=$current_fingerprint"
  echo "  $snapshot"
  exit 0
fi

snapshot_version="$(sed -n 's/^CACHE_VERSION=\([0-9][0-9]*\)$/\1/p' "$snapshot")"
snapshot_fingerprint="$(sed -n 's/^SCHEMA_FINGERPRINT=\([0-9a-f][0-9a-f]*\)$/\1/p' "$snapshot")"

if [[ -z "$snapshot_version" || -z "$snapshot_fingerprint" ]]; then
  echo "ERROR: snapshot must contain CACHE_VERSION=<n> and SCHEMA_FINGERPRINT=<sha256>" >&2
  exit 1
fi

failed=0
if [[ "$current_version" != "$snapshot_version" ]]; then
  echo "ERROR: Base cache schema snapshot CACHE_VERSION is stale." >&2
  echo "  current:  $current_version" >&2
  echo "  snapshot: $snapshot_version" >&2
  echo "  Run: $update_cmd" >&2
  failed=1
fi

if [[ "$current_fingerprint" != "$snapshot_fingerprint" ]]; then
  echo "ERROR: Base cache schema fingerprint changed." >&2
  echo "  current:  $current_fingerprint" >&2
  echo "  snapshot: $snapshot_fingerprint" >&2
  echo "  A file listed in $manifest changed (or the manifest itself did)." >&2
  echo "  Bump CACHE_VERSION in $precompile_rs, then run: $update_cmd" >&2
  failed=1
fi

if [[ "$failed" -ne 0 ]]; then
  exit 1
fi

echo "OK: Base cache schema fingerprint matches CACHE_VERSION $current_version"
