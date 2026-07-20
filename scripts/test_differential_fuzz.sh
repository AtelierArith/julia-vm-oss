#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/sjulia-diff-fuzz-test.XXXXXX")"
trap 'rm -rf "$tmpdir"' EXIT

sjulia_bin="${SJULIA_BIN:-target/dev-fast/sjulia}"
if [[ ! -x "$sjulia_bin" ]]; then
  cargo build --profile dev-fast -p subset_julia_vm --bin sjulia --features repl
fi

gen_a="$tmpdir/gen-a.tsv"
gen_b="$tmpdir/gen-b.tsv"
julia --startup-file=no scripts/differential_fuzz_generate.jl --seed 7 --count 4 > "$gen_a"
julia --startup-file=no scripts/differential_fuzz_generate.jl --seed 7 --count 4 > "$gen_b"
diff -u "$gen_a" "$gen_b"

if [[ "$(wc -l < "$gen_a" | tr -d ' ')" != "5" ]]; then
  echo "ERROR: generator should emit one header plus four cases" >&2
  cat "$gen_a" >&2
  exit 1
fi

if ! head -1 "$gen_a" | grep -qx $'case_seed\tcase_index\tdepth\tsource_b64'; then
  echo "ERROR: unexpected generator header" >&2
  head -1 "$gen_a" >&2
  exit 1
fi

gen_programs="$tmpdir/gen-programs.tsv"
julia --startup-file=no scripts/differential_fuzz_generate.jl --seed 9006 --count 12 --max-depth 5 > "$gen_programs"
python3 - "$gen_programs" <<'PY'
import base64
import sys

programs = []
for line in open(sys.argv[1], encoding="utf-8").read().splitlines()[1:]:
    fields = line.split("\t")
    assert len(fields) == 4, fields
    programs.append(base64.b64decode(fields[3]).decode("utf-8"))

joined = "\n".join(programs)
required = {
    "long-form function": "function " in joined,
    "if control flow": "\n    if " in joined or "\n        if " in joined,
    "for loop": "\n    for " in joined or "\n        for " in joined,
    "while loop": "\n    while " in joined or "\n        while " in joined,
}
missing = [name for name, present in required.items() if not present]
assert not missing, "generator missing program-level constructs: " + ", ".join(missing)
PY

runner_jsonl="$tmpdir/results.jsonl"
python3 scripts/differential_fuzz_runner.py \
  --seed 7 \
  --count 4 \
  --timeout-sec 20 \
  --sjulia-bin "$sjulia_bin" \
  --out-jsonl "$runner_jsonl" \
  --work-dir "$tmpdir/work"

python3 - "$runner_jsonl" <<'PY'
import json
import sys

rows = [json.loads(line) for line in open(sys.argv[1], encoding="utf-8")]
assert len(rows) == 4, rows
assert all(row["status"] == "pass" for row in rows), rows
assert all("fingerprint" in row for row in rows), rows
assert all(row["upstream_parse"]["status"] == "pass" for row in rows), rows
assert all(row["sjulia_parse"]["status"] == "pass" for row in rows), rows
PY

inject_jsonl="$tmpdir/injected.jsonl"
if python3 scripts/differential_fuzz_runner.py \
  --seed 7 \
  --count 3 \
  --timeout-sec 20 \
  --sjulia-bin "$sjulia_bin" \
  --out-jsonl "$inject_jsonl" \
  --work-dir "$tmpdir/injected-work" \
  --inject-known-mismatch; then
  echo "ERROR: injected mismatch should make the runner fail" >&2
  exit 1
fi

python3 - "$inject_jsonl" <<'PY'
import json
import sys

rows = [json.loads(line) for line in open(sys.argv[1], encoding="utf-8")]
failures = [row for row in rows if row["status"] == "fail"]
assert failures, rows
failure = failures[0]
assert failure["failure_kind"] == "stdout_mismatch", failure
assert failure["shrunk_source"].strip(), failure
assert failure["fingerprint"], failure
PY

report_md="$tmpdir/findings.md"
if python3 scripts/differential_fuzz_report_findings.py \
  --jsonl "$inject_jsonl" \
  --known docs/vm/DIFFERENTIAL_FUZZ_KNOWN_FINDINGS.tsv \
  --out-md "$report_md"; then
  echo "ERROR: injected finding should make the report command fail" >&2
  exit 1
fi
grep -q "New findings: 1" "$report_md"
grep -q "julia vs sjulia" "$report_md"

known="$tmpdir/known.tsv"
{
  printf 'fingerprint\tissue\tstatus\tnote\n'
  python3 - "$inject_jsonl" <<'PY'
import json
import sys

for line in open(sys.argv[1], encoding="utf-8"):
    row = json.loads(line)
    if row["status"] == "fail":
        print(f"{row['fingerprint']}\t#0\tknown\tself-test")
        break
PY
} > "$known"

python3 scripts/differential_fuzz_report_findings.py \
  --jsonl "$inject_jsonl" \
  --known "$known" \
  --out-md "$tmpdir/known-findings.md"
grep -q "New findings: 0" "$tmpdir/known-findings.md"

echo "OK: differential fuzz generator and runner self-test passed."
