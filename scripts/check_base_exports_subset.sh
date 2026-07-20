#!/usr/bin/env bash
# Keep SubsetJuliaVM's Base export metadata within upstream Julia's export surface.
set -euo pipefail

cd "$(dirname "$0")/.."

subset_exports="subset_julia_vm/src/julia/base/exports.jl"
upstream_exports="julia/base/exports.jl"

for path in "$subset_exports" "$upstream_exports"; do
  if [ ! -f "$path" ]; then
    echo "ERROR: required Base export manifest is missing: $path (Issue #11298)" >&2
    exit 1
  fi
done

python3 - "$subset_exports" "$upstream_exports" <<'PY'
import pathlib
import sys


def parse_exports(path):
    exports = set()
    collecting = False
    for raw_line in pathlib.Path(path).read_text(encoding="utf-8").splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if not line:
            continue

        if line == "export" or line.startswith("export "):
            names = line[len("export") :].strip()
        elif collecting:
            names = line
        else:
            continue

        for name in names.split(","):
            name = name.strip()
            if name:
                exports.add(name)
        collecting = line.endswith(",") or line == "export"

    if not exports:
        raise RuntimeError("parsed no exports from {}".format(path))
    return exports


subset_path, upstream_path = sys.argv[1:]
try:
    subset = parse_exports(subset_path)
    upstream = parse_exports(upstream_path)
except (OSError, UnicodeError, RuntimeError) as error:
    print("ERROR: failed to parse Base export manifests: {} (Issue #11298)".format(error), file=sys.stderr)
    sys.exit(1)

extras = sorted(subset - upstream)
if extras:
    print(
        "ERROR: SubsetJuliaVM Base exports include identifiers absent from upstream Julia: {} (Issue #11298)".format(
            ", ".join(extras)
        ),
        file=sys.stderr,
    )
    sys.exit(1)

print(
    "OK: SubsetJuliaVM's {} Base exports are contained in upstream Julia's {} exports (Issue #11298)".format(
        len(subset), len(upstream)
    )
)
PY
