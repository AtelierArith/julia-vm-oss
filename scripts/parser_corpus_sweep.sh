#!/usr/bin/env bash
# parser_corpus_sweep.sh — parser corpus differential sweep vs upstream Julia
# (Issue #8614 / #8635).
#
# Parses every .jl file in the upstream julia/ submodule corpus
# (base/, stdlib/, test/) with the sjulia parser (parse only — no lowering,
# no VM execution) and writes one TSV record per parse error / panic:
#
#   file <TAB> span <TAB> error_kind <TAB> snippet <TAB> message
#
# A summary (file counts, success rate, per-error-kind counts) is printed to
# stderr by the underlying `parse_corpus` bin (subset_julia_vm_parser).
# Baseline numbers and interpretation: docs/vm/PARSER_CORPUS_BASELINE.md.
#
# Usage:
#   bash scripts/parser_corpus_sweep.sh [--out FILE] [DIR ...]
#     --out FILE   TSV output path (default: target/parser_corpus/sweep.tsv)
#     DIR ...      corpus roots (default: julia/base julia/stdlib julia/test)

set -euo pipefail
cd "$(dirname "$0")/.."

OUT="target/parser_corpus/sweep.tsv"
DIRS=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out)
      [ $# -ge 2 ] || { echo "ERROR: --out requires a path" >&2; exit 2; }
      OUT="$2"
      shift 2
      ;;
    -h|--help)
      sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    -*)
      echo "ERROR: unknown flag $1 (see --help)" >&2
      exit 2
      ;;
    *)
      DIRS="$DIRS $1"
      shift
      ;;
  esac
done
if [ -z "$DIRS" ]; then
  DIRS="julia/base julia/stdlib julia/test"
fi

for dir in $DIRS; do
  if [ ! -d "$dir" ]; then
    echo "ERROR: corpus dir '$dir' not found. For the default corpus run:" >&2
    echo "  git submodule update --init julia" >&2
    exit 1
  fi
done

cargo build --release -p subset_julia_vm_parser --bin parse_corpus

mkdir -p "$(dirname "$OUT")"
# LC_ALL=C sort keeps the sweep order (and therefore the TSV) deterministic.
# shellcheck disable=SC2086  # DIRS is intentionally word-split
find $DIRS -type f -name '*.jl' | LC_ALL=C sort \
  | ./target/release/parse_corpus --files-from - > "$OUT"

echo "parser_corpus_sweep: wrote $OUT" >&2
