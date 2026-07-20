# Differential Fuzzing

This page documents the execution-differential fuzzing loop for Issues #8716
and #9006, parent #8692. The generator emits deterministic upstream-valid
programs, then the runner compares parse success and execution output against
sjulia.

## Generator

`scripts/differential_fuzz_generate.jl` is the source of truth for seed to
program generation. It emits TSV rows:

```text
case_seed	case_index	depth	source_b64
```

The grammar covers:

- typed numeric literals (`Int64(...)`, `Float64(...)`);
- binary `+`, `-`, `*` expressions;
- simple function composition through `f8716` and `g8716`;
- a `let` wrapper binding `x` and `y`;
- long-form function definitions;
- bounded `if`, `for`, and `while` control-flow templates.

The control-flow templates deliberately use small integer loop bounds and a
fixed helper surface so nightly fuzzing explores parser/lowering/scope shape
without producing unbounded execution or unsupported-feature floods.

Generation is deterministic: the same `--seed`, `--count`, and `--max-depth`
produce byte-identical TSV output.

## Runner

`scripts/differential_fuzz_runner.py` orchestrates the loop:

1. generate cases with the Julia generator;
2. parse each source with upstream Julia (`Meta.parseall`);
3. parse each source with sjulia (`--dump-ast`);
4. classify parser failures as `sjulia_parse_error` or
   `sjulia_parse_timeout`;
5. run parser-clean sources under upstream Julia first;
6. skip cases that are not upstream-valid at execution time;
7. run upstream-valid cases under sjulia;
8. classify execution failures as `stdout_mismatch`, `sjulia_error`, or
   `sjulia_timeout`;
9. write one JSON object per case to `--out-jsonl`.

Each row includes the seed, case index, generated source, upstream/sjulia
parse status, upstream/sjulia stdout/stderr/status, a stable fingerprint, and
`shrunk_source` for failures.

## Shrinking

For a failing case, the runner asks the generator for AST-level shrink
candidates (`--mode shrinks`) and keeps the smallest candidate that reproduces
the same failure kind while remaining upstream-valid. This is a deliberately
small reducer, but it already gives a seed-reproducible MWE for the initial
numeric layer.

## Local commands

```bash
julia --startup-file=no scripts/differential_fuzz_generate.jl --seed 1 --count 5

cargo build --profile dev-fast -p subset_julia_vm --bin sjulia --features repl
SJULIA_BIN=target/dev-fast/sjulia \
  python3 scripts/differential_fuzz_runner.py \
    --seed 1 \
    --count 20 \
    --timeout-sec 5 \
    --out-jsonl target/differential-fuzz/results.jsonl

bash scripts/test_differential_fuzz.sh
```

`--inject-known-mismatch` is a self-test hook only. It perturbs the first sjulia
result after execution so the test can verify mismatch classification,
fingerprinting, and shrink artifact emission without relying on a real current
semantic bug.

## Nightly Operation

Issue #8717 wires the loop into `.github/workflows/nightly-gates.yml` as
`differential-fuzz`. The job uses the UTC date as the seed and runs with:

- `--budget-sec 900` (15 minutes wall-clock fuzz budget);
- `--count 100000` (high ceiling; the budget normally stops first);
- `--timeout-sec 5` per generated program;
- `target/dev-fast/sjulia` to keep the job inside the nightly budget.

The runner writes `target/differential-fuzz/results.jsonl`. Then
`scripts/differential_fuzz_report_findings.py` filters out known fingerprints,
writes `target/differential-fuzz/findings.md`, and creates GitHub Issues for new
fingerprints when `--create-issues` is set.

## Finding Lifecycle

Known/triaged findings live in
`docs/vm/DIFFERENTIAL_FUZZ_KNOWN_FINDINGS.tsv`:

```text
fingerprint	issue	status	note
```

The report helper suppresses duplicates from both this TSV and existing GitHub
issues containing the fingerprint. After triage:

1. decide whether the finding is `bug` (sjulia runs but differs) or
   `unsupported-feature` (sjulia cannot run valid upstream Julia);
2. keep or relabel the generated Issue with the correct label;
3. add the fingerprint, issue number, and status to the TSV in the fix/triage PR;
4. expand the generator grammar only after one month of stable signal, as owned
   by parent #8692.
