#!/usr/bin/env bash
# check_build_locked.sh — audit that CI workflows and build scripts pass
# --locked to cargo build / cargo nextest run / wasm-pack build / cargo ndk
# build so dependency resolution is pinned to Cargo.lock (Issue #9002).
#
# Exits 1 if any offending line is found; exits 0 otherwise.
#
# Scanned files:
#   .github/workflows/*.yml
#   build.sh
#   build_android.sh
#   mobile/scripts/build_android.sh
#   scripts/wasm_build_with_cache.sh
#   scripts/test_with_cache.sh
#   scripts/test_aot.sh
#
# Exemptions (intentional unlocked calls):
#   - cargo install       (installs tools; --locked there is optional/separate)
#   - cargo check         (type-check only, not a reproducible artifact)
#   - cargo clippy        (lint only, not a reproducible artifact)
#   - cargo bench         (benchmark driver, not a published artifact)
#   - wasm-pack build ... executed via wasm_build_with_cache.sh exec at end of
#     the script (the --locked flag is injected by that script directly into the
#     wasm_pack_args array before exec, so the source line itself looks like
#     `exec wasm-pack build "${wasm_pack_args[@]}"` — the array contents are
#     not statically inspectable; we exempt this exec form)
#   - Lines that are comments (start with optional whitespace + #)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Files to scan.
SCANNED_FILES=(
  "$REPO_ROOT/.github/workflows/ci.yml"
  "$REPO_ROOT/.github/workflows/platform-builds.yml"
  "$REPO_ROOT/.github/workflows/nightly-gates.yml"
  "$REPO_ROOT/.github/workflows/main-full.yml"
  "$REPO_ROOT/.github/workflows/pr-fast.yml"
  "$REPO_ROOT/.github/workflows/release.yml"
  "$REPO_ROOT/build.sh"
  "$REPO_ROOT/build_android.sh"
  "$REPO_ROOT/mobile/scripts/build_android.sh"
  "$REPO_ROOT/scripts/wasm_build_with_cache.sh"
  "$REPO_ROOT/scripts/test_with_cache.sh"
  "$REPO_ROOT/scripts/test_aot.sh"
)

# Optional explicit paths are used by the audit negative self-test. Relative
# paths are interpreted from the repository root, matching normal invocation.
if [[ "$#" -gt 0 ]]; then
  SCANNED_FILES=()
  for file in "$@"; do
    if [[ "$file" != /* ]]; then
      file="$REPO_ROOT/$file"
    fi
    if [[ ! -e "$file" ]]; then
      echo "FAIL: explicit scan target does not exist: $file" >&2
      exit 1
    fi
    if [[ ! -f "$file" ]]; then
      echo "FAIL: explicit scan target is not a regular file: $file" >&2
      exit 1
    fi
    SCANNED_FILES+=("$file")
  done
fi

# Pattern: a non-comment line containing one of the build commands but NOT
# containing --locked.  We check four command forms:
#   cargo build ...
#   cargo nextest run ...
#   wasm-pack build ...
#   cargo ndk ... build ...
#
# False-negative risk for multi-line YAML run blocks: the --locked flag may
# appear on a continuation line.  To handle this we join continuation lines
# (lines ending in \) and check the combined logical line.

found_violations=0

# Join shell continuation lines without relying on sed dialect extensions.
# This stays compatible with the repository's Bash 3.2 floor and with both
# BSD/macOS and GNU/Linux userlands (Issue #11257).
join_continuation_lines() {
  local file="$1" line pending="" trailing backslash_count
  if [[ ! -r "$file" ]]; then
    echo "FAIL: cannot read scanned file: $file" >&2
    return 1
  fi

  while IFS= read -r line || [[ -n "$line" ]]; do
    # read -r retains CR from CRLF input. It is a line terminator here, not
    # command content, and must be removed before examining the final byte.
    line="${line%$'\r'}"

    trailing="$line"
    backslash_count=0
    while [[ "$trailing" == *\\ ]]; do
      trailing="${trailing%\\}"
      backslash_count=$(( backslash_count + 1 ))
    done

    # A newline is escaped only when its immediately preceding backslash run
    # is odd. With an even run all backslashes are themselves escaped, so the
    # next physical line must remain a separate logical line.
    if [[ $(( backslash_count % 2 )) -eq 1 ]]; then
      pending="${pending}${line%\\}"
    else
      printf '%s%s\n' "$pending" "$line"
      pending=""
    fi
  done < "$file"

  if [[ -n "$pending" ]]; then
    printf '%s\n' "$pending"
  fi
}

check_file() {
  local file="$1"
  if [[ ! -e "$file" ]]; then
    # release.yml may be absent on old branches — it is the sole optional
    # built-in target. An existing non-regular path still fails closed below.
    if [[ "$file" == "$REPO_ROOT/.github/workflows/release.yml" ]]; then
      return
    fi
    echo "FAIL: required built-in scan target is missing: $file" >&2
    return 1
  fi
  if [[ ! -f "$file" ]]; then
    echo "FAIL: required built-in scan target is not a regular file: $file" >&2
    return 1
  fi

  # Read the file and join shell line continuations (lines ending in \) so
  # that `cargo build \` `  --locked` is seen as a single logical line.
  local joined
  if ! joined="$(join_continuation_lines "$file")"; then
    echo "FAIL: could not normalize shell continuations in $file" >&2
    return 1
  fi

  local lineno=0
  while IFS= read -r line; do
    lineno=$(( lineno + 1 ))

    # Skip comment lines (shell # or YAML # after optional whitespace).
    if echo "$line" | grep -qE '^[[:space:]]*#'; then
      continue
    fi

    # Skip echo/print lines (the command appears only inside a quoted string).
    if echo "$line" | grep -qE '^[[:space:]]*(echo|printf)[[:space:]]'; then
      continue
    fi

    # Check: cargo build (not cargo build.rs, install, check, clippy, bench)
    if echo "$line" | grep -qE '(^|[[:space:]])cargo build([[:space:]]|$)'; then
      if ! echo "$line" | grep -q '\-\-locked'; then
        echo "MISSING --locked: $file (logical line near $lineno):"
        echo "  $line"
        found_violations=1
      fi
    fi

    # Check: cargo nextest run
    if echo "$line" | grep -qE '(^|[[:space:]])cargo nextest run([[:space:]]|$)'; then
      if ! echo "$line" | grep -q '\-\-locked'; then
        echo "MISSING --locked: $file (logical line near $lineno):"
        echo "  $line"
        found_violations=1
      fi
    fi

    # Check: wasm-pack build (but not `exec wasm-pack build` where args come
    # from a variable array — that form is inside wasm_build_with_cache.sh and
    # the --locked is injected at runtime).
    if echo "$line" | grep -qE '(^|[[:space:]])wasm-pack build([[:space:]]|$)'; then
      if echo "$line" | grep -qE '(^|[[:space:]])exec[[:space:]]'; then
        # Array-expansion exec form — skip (runtime injection).
        true
      elif ! echo "$line" | grep -q '\-\-locked'; then
        echo "MISSING --locked: $file (logical line near $lineno):"
        echo "  $line"
        found_violations=1
      fi
    fi

    # Check: cargo ndk ... build
    if echo "$line" | grep -qE '(^|[[:space:]])cargo ndk([[:space:]]).*build([[:space:]]|$)'; then
      if ! echo "$line" | grep -q '\-\-locked'; then
        echo "MISSING --locked: $file (logical line near $lineno):"
        echo "  $line"
        found_violations=1
      fi
    fi

  done <<< "$joined"
}

for f in "${SCANNED_FILES[@]}"; do
  check_file "$f"
done

if [[ "$found_violations" -eq 0 ]]; then
  echo "OK: all scanned cargo build / nextest run / wasm-pack build / cargo ndk build calls include --locked"
  exit 0
else
  echo ""
  echo "Add --locked to each offending call so dependency resolution is pinned"
  echo "to the committed Cargo.lock, making builds reproducible (Issue #9002)."
  exit 1
fi
