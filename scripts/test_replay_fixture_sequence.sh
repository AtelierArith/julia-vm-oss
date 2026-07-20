#!/usr/bin/env bash
# Smoke-test replay_fixture_sequence.sh journal parsing without invoking Cargo.

set -euo pipefail
cd "$(dirname "$0")/.."

tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/fixture-replay-test.XXXXXX")"
trap 'rm -rf "$tmpdir"' EXIT

journal="$tmpdir/fixtures.jsonl"
cat > "$journal" <<'JSONL'
{"test":"alpha_first","fixture":"alpha/first.jl","fixture_path":"/repo/subset_julia_vm/tests/fixtures/alpha/first.jl"}
{"test":"beta_second","fixture":"beta/second.jl","fixture_path":"/repo/subset_julia_vm/tests/fixtures/beta/second.jl"}
{"test":"gamma_fail","fixture":"gamma/fail.jl","fixture_path":"/repo/subset_julia_vm/tests/fixtures/gamma/fail.jl"}
JSONL

out="$tmpdir/plan.txt"
scripts/replay_fixture_sequence.sh --plan-only "$journal" "gamma/fail.jl" > "$out"

grep -q '^failing_fixture=gamma/fail\.jl$' "$out"
grep -q '^predecessor_count=2$' "$out"
grep -q '^candidate_count=3$' "$out"
grep -q '^runner_test=fixture_sequence_replay_8709_from_env$' "$out"
