#!/usr/bin/env bash
set -euo pipefail

SJULIA_BIN="${SJULIA_BIN:-target/release/sjulia}"
BASELINE="${ERROR_TRACE_PARITY_BASELINE:-docs/vm/ERROR_TRACE_PARITY_BASELINE.tsv}"
WORKDIR="${ERROR_TRACE_PARITY_WORKDIR:-$(mktemp -d "${TMPDIR:-/tmp}/sjulia-error-trace.XXXXXX")}"

cleanup() {
  if [ -z "${ERROR_TRACE_PARITY_WORKDIR:-}" ]; then
    rm -rf "$WORKDIR"
  fi
}
trap cleanup EXIT

if ! command -v julia >/dev/null 2>&1; then
  echo "ERROR: julia not found on PATH." >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "ERROR: python3 not found on PATH." >&2
  exit 1
fi

if [ ! -x "$SJULIA_BIN" ]; then
  echo "ERROR: sjulia binary not executable: $SJULIA_BIN" >&2
  echo "Build it with: cargo build --release -p subset_julia_vm --bin sjulia --features repl" >&2
  exit 1
fi

if [ ! -f "$BASELINE" ]; then
  echo "ERROR: baseline not found: $BASELINE" >&2
  exit 1
fi

mkdir -p "$WORKDIR"

cat > "$WORKDIR/method.jl" <<'JL'
function outer_8713_method()
    middle_8713_method()
end
function middle_8713_method()
    inner_8713_method()
end
function inner_8713_method()
    missing_method_for_8713(1)
end
outer_8713_method()
JL

cat > "$WORKDIR/domain.jl" <<'JL'
function outer_8713_domain()
    middle_8713_domain()
end
function middle_8713_domain()
    inner_8713_domain()
end
function inner_8713_domain()
    sqrt(-1.0)
end
outer_8713_domain()
JL

run_case() {
  runner="$1"
  case_name="$2"
  output="$3"
  status_file="$4"
  set +e
  if [ "$runner" = "julia" ]; then
    julia --startup-file=no "$WORKDIR/$case_name.jl" > "$output" 2>&1
  else
    "$SJULIA_BIN" "$WORKDIR/$case_name.jl" > "$output" 2>&1
  fi
  exit_status=$?
  set -e
  printf '%s\n' "$exit_status" > "$status_file"
}

for case_name in method domain; do
  run_case julia "$case_name" "$WORKDIR/upstream-$case_name.out" "$WORKDIR/upstream-$case_name.status"
  run_case sjulia "$case_name" "$WORKDIR/sjulia-$case_name.out" "$WORKDIR/sjulia-$case_name.status"
done

python3 - "$WORKDIR" "$BASELINE" <<'PY'
import csv
import pathlib
import re
import sys

workdir = pathlib.Path(sys.argv[1])
baseline_path = pathlib.Path(sys.argv[2])

target_re = re.compile(r"\b(inner_8713_(?:method|domain)|middle_8713_(?:method|domain)|outer_8713_(?:method|domain))\b")
line_re = re.compile(r":(\d+)(?:\D|$)")
order = {"inner": 0, "middle": 1, "outer": 2}


def read_status(path: pathlib.Path) -> int:
    return int(path.read_text(encoding="utf-8").strip())


def user_frames(text: str) -> list[tuple[str, int | None]]:
    lines = text.splitlines()
    frames: list[tuple[str, int | None]] = []
    for idx, line in enumerate(lines):
        match = target_re.search(line)
        if not match:
            continue
        fn = match.group(1)
        source_line = None
        for lookahead in lines[idx + 1 : idx + 3]:
            line_match = line_re.search(lookahead)
            if line_match:
                source_line = int(line_match.group(1))
                break
        frames.append((fn, source_line))
    return frames


def ordered(frames: list[tuple[str, int | None]]) -> list[tuple[str, int | None]]:
    def key(item: tuple[str, int | None]) -> int:
        return order[item[0].split("_", 1)[0]]

    return sorted(frames, key=key)


def compact(frames: list[tuple[str, int | None]]) -> str:
    return ",".join(f"{fn}:{line if line is not None else '?'}" for fn, line in frames)


with baseline_path.open(encoding="utf-8", newline="") as f:
    baseline = {row["case"]: row for row in csv.DictReader(f, delimiter="\t")}

rows = []
failures = []
for case_name in ("method", "domain"):
    upstream_text = (workdir / f"upstream-{case_name}.out").read_text(encoding="utf-8")
    sjulia_text = (workdir / f"sjulia-{case_name}.out").read_text(encoding="utf-8")
    upstream_status = read_status(workdir / f"upstream-{case_name}.status")
    sjulia_status = read_status(workdir / f"sjulia-{case_name}.status")
    upstream_frames = ordered(user_frames(upstream_text))
    sjulia_frames = ordered(user_frames(sjulia_text))
    function_names_match = [fn for fn, _ in upstream_frames] == [fn for fn, _ in sjulia_frames]
    line_match = upstream_frames == sjulia_frames
    format_match = upstream_text == sjulia_text

    row = {
        "case": case_name,
        "upstream_status": str(upstream_status),
        "sjulia_status": str(sjulia_status),
        "user_frame_count": str(len(sjulia_frames)),
        "line_match": "yes" if line_match else "no",
        "function_names_match": "yes" if function_names_match else "no",
        "format_match": "yes" if format_match else "no",
        "upstream_user_frames": compact(upstream_frames),
        "sjulia_user_frames": compact(sjulia_frames),
    }
    rows.append(row)

    expected = baseline.get(case_name)
    if expected is None:
        failures.append(f"{case_name}: missing baseline row")
        continue
    if upstream_status == 0 or sjulia_status == 0:
        failures.append(f"{case_name}: expected both runners to fail, got upstream={upstream_status} sjulia={sjulia_status}")
    if len(sjulia_frames) < int(expected["min_user_frames"]):
        failures.append(f"{case_name}: sjulia user frame count {len(sjulia_frames)} below baseline {expected['min_user_frames']}")
    for col in ("line_match", "function_names_match", "format_match"):
        if row[col] != expected[col]:
            failures.append(f"{case_name}: {col} is {row[col]}, baseline expects {expected[col]}")

writer = csv.DictWriter(
    sys.stdout,
    fieldnames=[
        "case",
        "upstream_status",
        "sjulia_status",
        "user_frame_count",
        "line_match",
        "function_names_match",
        "format_match",
        "upstream_user_frames",
        "sjulia_user_frames",
    ],
    delimiter="\t",
    lineterminator="\n",
)
writer.writeheader()
writer.writerows(rows)

if failures:
    print("ERROR: error trace parity ratchet failed:", file=sys.stderr)
    for failure in failures:
        print(f"  - {failure}", file=sys.stderr)
    sys.exit(1)
PY
