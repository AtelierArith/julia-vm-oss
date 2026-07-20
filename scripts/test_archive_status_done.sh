#!/usr/bin/env bash
# Regression self-test for multi-batch STATUS/DONE archive chronology (Issue #11263).

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/docs/vm/archive" "$TMP/scripts"
cp "$ROOT/scripts/archive_status_done.sh" "$TMP/scripts/"
cp "$ROOT/scripts/check_status_done_archive_budget.sh" "$TMP/scripts/"

write_live_file() {
  file="$1"
  shift
  {
    printf '# self-test %s\n\n---\n\n' "$file"
    for date in "$@"; do
      printf '## 最新対応 (%s)\n\nbody-%s-%s\n\n' "$date" "$file" "$date"
    done
  } > "$TMP/docs/vm/$file.md"
}

archive_dates() {
  sed -n 's/^## 最新対応 (\([0-9-]*\))$/\1/p' "$1"
}

snapshot_sections() {
  snapshot="$1"
  shift
  python3 - "$TMP" "$snapshot" "$@" <<'PY'
import base64
import json
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])
snapshot = Path(sys.argv[2])
wanted = set(sys.argv[3:])
section_re = re.compile(r"^## 最新対応 \((\d{4}-\d{2}-\d{2})\)\n".encode(), re.MULTILINE)
sections = {}
for name in ("STATUS", "DONE"):
    data = (root / "docs" / "vm" / f"{name}.md").read_bytes()
    matches = list(section_re.finditer(data))
    for index, match in enumerate(matches):
        date = match.group(1).decode("ascii")
        if date not in wanted:
            continue
        end = matches[index + 1].start() if index + 1 < len(matches) else len(data)
        sections[f"{name}:{date}"] = base64.b64encode(data[match.start():end]).decode("ascii")
expected_keys = {f"{name}:{date}" for name in ("STATUS", "DONE") for date in wanted}
assert sections.keys() == expected_keys, (sections.keys(), expected_keys)
snapshot.write_text(json.dumps(sections, sort_keys=True), encoding="utf-8")
PY
}

compare_snapshot() {
  snapshot="$1"
  python3 - "$TMP" "$snapshot" <<'PY'
import base64
import json
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])
expected = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
section_re = re.compile(r"^## 最新対応 \((\d{4}-\d{2}-\d{2})\)\n".encode(), re.MULTILINE)
actual = {}
for name in ("STATUS", "DONE"):
    archive = root / "docs" / "vm" / "archive" / f"{name}-2026.md"
    data = archive.read_bytes()
    matches = list(section_re.finditer(data))
    for index, match in enumerate(matches):
        key = f"{name}:{match.group(1).decode('ascii')}"
        if key not in expected:
            continue
        end = matches[index + 1].start() if index + 1 < len(matches) else len(data)
        actual[key] = base64.b64encode(data[match.start():end]).decode("ascii")
if actual != expected:
    missing = sorted(expected.keys() - actual.keys())
    changed = sorted(key for key in expected.keys() & actual.keys() if expected[key] != actual[key])
    raise SystemExit(f"FAIL: archived section bytes changed; missing={missing}, changed={changed}")
PY
}

# Batch one moves 2026-01-04..01 while retaining the newest section live.
write_live_file STATUS 2026-01-05 2026-01-04 2026-01-03 2026-01-02 2026-01-01
write_live_file DONE 2026-01-05 2026-01-04 2026-01-03 2026-01-02 2026-01-01
snapshot_sections "$TMP/batch1.json" 2026-01-04 2026-01-03 2026-01-02 2026-01-01

set +e
wrapper_out="$(cd "$TMP" && bash scripts/check_status_done_archive_budget.sh --max-lines 9 2>&1)"
wrapper_code=$?
set -e
if [ "$wrapper_code" -ne 2 ] || ! printf '%s\n' "$wrapper_out" | grep -qF 'fixed 3000-line invariant'; then
  echo "FAIL: read-only archive budget wrapper accepted an arbitrary threshold" >&2
  exit 1
fi

(cd "$TMP" && bash scripts/archive_status_done.sh --max-lines 9)

# Add two newer live sections. Batch two moves 2026-01-06 and 2026-01-05 and
# must merge them ahead of the older archive without reversing either batch.
write_live_file STATUS 2026-01-07 2026-01-06 2026-01-05
write_live_file DONE 2026-01-07 2026-01-06 2026-01-05
snapshot_sections "$TMP/batch2.json" 2026-01-06 2026-01-05
(cd "$TMP" && bash scripts/archive_status_done.sh --max-lines 9)

# Compare full byte slices, not marker presence: this includes the trailing
# blank lines of a section that originally reached EOF before being reordered.
compare_snapshot "$TMP/batch1.json"
compare_snapshot "$TMP/batch2.json"

expected_dates='2026-01-06
2026-01-05
2026-01-04
2026-01-03
2026-01-02
2026-01-01'

for name in STATUS DONE; do
  archive="$TMP/docs/vm/archive/$name-2026.md"
  actual_dates="$(archive_dates "$archive")"
  if [ "$actual_dates" != "$expected_dates" ]; then
    echo "FAIL: $name archive chronology is not newest-to-oldest after multiple batches" >&2
    printf 'expected:\n%s\nactual:\n%s\n' "$expected_dates" "$actual_dates" >&2
    exit 1
  fi
done

echo "OK: multi-batch STATUS/DONE archives stay newest-to-oldest and preserve exact section bytes."
