#!/usr/bin/env bash
# Unit smoke for scripts/check_panic_free_ratchet.sh without scanning the repo.

set -euo pipefail

cd "$(dirname "$0")/.."

tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/panic-free-ratchet-test.XXXXXX")"
trap 'rm -rf "$tmpdir"' EXIT

mkdir -p "$tmpdir/src"
cat > "$tmpdir/src/sample.rs" <<'RS'
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

fn sample(value: Option<i64>) -> i64 {
    value.unwrap()
}
RS

cat > "$tmpdir/baseline.tsv" <<'TSV'
metric	module	baseline
unwrap_call	src	1
expect_call	src	0
panic_macro	src	0
todo_macro	src	0
unimplemented_macro	src	0
TSV

cat > "$tmpdir/deny.tsv" <<'TSV'
file	unwrap_used	expect_used	reason
src/sample.rs	true	true	test fixture
TSV

PANIC_RATCHET_ROOTS="$tmpdir/src" \
PANIC_RATCHET_BASELINE="$tmpdir/baseline.tsv" \
PANIC_DENY_MODULES="$tmpdir/deny.tsv" \
  bash scripts/check_panic_free_ratchet.sh >/tmp/panic-ratchet-pass.out

cat > "$tmpdir/baseline.tsv" <<'TSV'
metric	module	baseline
unwrap_call	src	0
expect_call	src	0
panic_macro	src	0
todo_macro	src	0
unimplemented_macro	src	0
TSV

if PANIC_RATCHET_ROOTS="$tmpdir/src" \
   PANIC_RATCHET_BASELINE="$tmpdir/baseline.tsv" \
   PANIC_DENY_MODULES="$tmpdir/deny.tsv" \
     bash scripts/check_panic_free_ratchet.sh >"$tmpdir/fail.out" 2>"$tmpdir/fail.err"; then
  echo "ERROR: panic-free ratchet accepted a count above baseline" >&2
  exit 1
fi

grep -q "unwrap_call" "$tmpdir/fail.err"
