#!/usr/bin/env bash
# north_star_report.sh — collect the North Star metrics (docs/vm/NORTH_STAR.md)
# and append a dated record to benchmarks/results/north_star/ (Issue #8682,
# parent #8680; metric definitions + baseline: Issue #8681).
#
# Collection is delegated to the existing per-metric machinery so manual runs
# and CI produce identical records:
#   NS-1  scripts/fixture_julia_parity.sh (full fixture sweep)
#   NS-2  scripts/parser_corpus_sweep.sh (parse_corpus bin)
#   NS-3  scripts/ios_samples_e2e.py — macOS only
#         (manual/monthly; pass --ns3-report report.txt, or legacy --ns3 PASS/TOTAL)
#   NS-4  benchmarks/run_benchmarks.sh (3-tier reproducible benchmark)
#   NS-5  cache-embedded sjulia cold start
#   NS-6  latest main-full CI run via gh (n/a without gh/token)
#   NS-7  docs/vm/WORKAROUNDS.md W-ID count +
#         scripts/check_structural_debt_inventory.sh inventory +
#         scripts/loc_report.sh --variants-only (NS-7 (c) Instr/TypedLoopOp
#         variant counts, Issue #10817 / #10899). NS-7 (d) (semantic-
#         implementation-lanes-per-construct) stays manual-only — see
#         docs/vm/NORTH_STAR.md NS-7 (d); this script only prints a pointer.
#
# Outputs:
#   benchmarks/results/north_star/YYYY-MM-DD.md   human-readable record
#   benchmarks/results/north_star/north_star.tsv  one machine-readable row/run
#
# Usage:
#   bash scripts/north_star_report.sh [--skip ns1,ns2,ns4,ns5,ns6,ns7c] \
#       [--date YYYY-MM-DD] [--ns3-report report.txt] [--ns3 PASS/TOTAL] \
#       [--check-regression]
#
# By default the time-series metrics NS-4/NS-5/NS-6 are collected only when NOT
# skipped; on a busy shared host prefer `--skip ns4,ns5,ns6` and let CI capture
# them (NORTH_STAR.md:「時間系はローカル計測を provisional とし CI 計測を正」).
# NS-7 (c) is cheap (two enum scans) and runs by default; `--skip ns7c` is
# available for symmetry with the other metrics but should rarely be needed.
#
# --check-regression compares the new row against the previous TSV row and
# exits 3 on a trend regression (NORTH_STAR.md「後退の扱い」):
#   NS-1 rate down, NS-2 rate down (same corpus commit only),
#   NS-7 workaround count up               → hard regressions
#   NS-4 / NS-5 worse than previous by >20% → performance regressions
# NS-7 structural-debt ratchets are enforced by
# check_structural_debt_inventory.sh itself and are only recorded here.
# NS-7 (c) (Instr/TypedLoopOp variant counts) is record-only and is NOT part
# of --check-regression: fused-op variant growth can be a legitimate
# optimization (Issue #10817), so an increase is not itself a regression —
# only that new variants should go through the #10814 metadata-derivation
# precondition, which is a PR-review concern, not a nightly gate.
set -uo pipefail
cd "$(dirname "$0")/.."

SKIP=""
DATE="$(date +%F)"
NS3_MANUAL=""
NS3_REPORT=""
CHECK_REGRESSION=0
while [ $# -gt 0 ]; do
  case "$1" in
    --skip) SKIP="$2"; shift 2 ;;
    --date) DATE="$2"; shift 2 ;;
    --ns3) NS3_MANUAL="$2"; shift 2 ;;
    --ns3-report) NS3_REPORT="$2"; shift 2 ;;
    --check-regression) CHECK_REGRESSION=1; shift ;;
    -h|--help) sed -n '2,48p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "ERROR: unknown arg $1 (see --help)" >&2; exit 2 ;;
  esac
done

skip() { case ",$SKIP," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }

OUT_DIR="benchmarks/results/north_star"
mkdir -p "$OUT_DIR"
MD="$OUT_DIR/$DATE.md"
TSV="$OUT_DIR/north_star.tsv"

COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
JULIA_VER="$(julia --version 2>/dev/null | awk '{print $3}' || echo n/a)"
CORPUS_COMMIT="$(git -C julia rev-parse --short HEAD 2>/dev/null || echo n/a)"
LOAD="$(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null || echo n/a)"
ENVIRONMENT="${NORTH_STAR_ENV:-local}"
NPROC="$(nproc 2>/dev/null || echo 1)"

echo "north_star_report: date=$DATE commit=$COMMIT env=$ENVIRONMENT load=$LOAD" >&2

# ---------------------------------------------------------------- NS-1
NS1_PASS=n/a NS1_TOTAL=n/a NS1_RATE=n/a
if ! skip ns1; then
  echo "== NS-1 parity fixture sweep ==" >&2
  if [ ! -x target/release/sjulia ]; then
    cargo build --release -p subset_julia_vm --bin sjulia --features repl
  fi
  sweep_file="$(mktemp)"
  find subset_julia_vm/tests/fixtures -name '*.jl' -type f -not -name '.*' \
    | sort \
    | xargs -P "$NPROC" -I{} bash -c \
        'bash scripts/fixture_julia_parity.sh "{}" >/dev/null 2>&1 && echo "PASS {}" || echo "FAIL {}"' \
    > "$sweep_file"
  NS1_PASS=$(grep -c '^PASS' "$sweep_file" || true)
  NS1_TOTAL=$(wc -l < "$sweep_file" | tr -d ' ')
  NS1_RATE=$(awk -v p="$NS1_PASS" -v t="$NS1_TOTAL" 'BEGIN{printf "%.2f", t ? 100*p/t : 0}')
  grep '^FAIL' "$sweep_file" | awk '{print $2}' | sort > "$OUT_DIR/$DATE-parity-failures.txt" || true
  rm -f "$sweep_file"
  echo "NS-1: $NS1_PASS/$NS1_TOTAL = $NS1_RATE%" >&2
fi

# ---------------------------------------------------------------- NS-2
NS2_CLEAN=n/a NS2_TOTAL=n/a NS2_RATE=n/a NS2_PANICS=n/a
if ! skip ns2; then
  echo "== NS-2 parser corpus sweep ==" >&2
  ns2_log="$(mktemp)"
  if bash scripts/parser_corpus_sweep.sh >/dev/null 2>"$ns2_log"; then
    # parse_corpus summary (stderr), e.g.:
    #  parse_corpus: 673 files | ok 422 (62.70%) | with parse errors 251 | panicked 0 | ...
    line="$(grep -E 'parse_corpus: [0-9]+ files' "$ns2_log" | head -1)"
    NS2_TOTAL=$(echo "$line" | sed -nE 's/.*parse_corpus: ([0-9]+) files.*/\1/p')
    NS2_CLEAN=$(echo "$line" | sed -nE 's/.*\| ok ([0-9]+) .*/\1/p')
    NS2_PANICS=$(echo "$line" | sed -nE 's/.*panicked ([0-9]+).*/\1/p')
    if [ -n "${NS2_TOTAL:-}" ] && [ -n "${NS2_CLEAN:-}" ]; then
      NS2_RATE=$(awk -v c="$NS2_CLEAN" -v t="$NS2_TOTAL" 'BEGIN{printf "%.2f", t ? 100*c/t : 0}')
    fi
  else
    echo "WARN: parser_corpus_sweep failed (julia/ submodule checked out?)" >&2
  fi
  rm -f "$ns2_log"
  echo "NS-2: ${NS2_CLEAN}/${NS2_TOTAL} = ${NS2_RATE}% panics=${NS2_PANICS}" >&2
fi

# ---------------------------------------------------------------- NS-3 (manual)
NS3_RATE=n/a NS3_INFRA_RATE=n/a NS3_DETAIL="未計測(macOS 依存・手動月次)"
if [ -n "$NS3_REPORT" ]; then
  ns3_summary="$(python3 scripts/ios_e2e_report.py --summary "$NS3_REPORT")"
  kv() { echo "$ns3_summary" | sed -nE "s/^$1=(.*)$/\\1/p"; }
  ns3_pass="$(kv sample_pass)"
  ns3_fail="$(kv sample_fail)"
  ns3_infra="$(kv infra_failure)"
  ns3_sample_total="$(kv sample_total)"
  ns3_total="$(kv total)"
  NS3_RATE="$(kv sample_rate)"
  NS3_INFRA_RATE="$(kv infra_rate)"
  NS3_DETAIL="report ${ns3_pass}/${ns3_sample_total} sample-pass = ${NS3_RATE}% (infra ${ns3_infra}/${ns3_total} = ${NS3_INFRA_RATE}%, source ${NS3_REPORT})"
elif [ -n "$NS3_MANUAL" ]; then
  p="${NS3_MANUAL%%/*}"; t="${NS3_MANUAL##*/}"
  NS3_RATE=$(awk -v p="$p" -v t="$t" 'BEGIN{printf "%.2f", t ? 100*p/t : 0}')
  NS3_DETAIL="手動計測 $NS3_MANUAL = ${NS3_RATE}% (infra n/a; prefer --ns3-report)"
fi

# ---------------------------------------------------------------- NS-4
NS4_CLI=n/a NS4_BC=n/a
if ! skip ns4; then
  echo "== NS-4 representative benchmark ==" >&2
  bench_dir="$(mktemp -d)"
  if RESULTS_DIR="$bench_dir" RUNS="${RUNS:-5}" ./benchmarks/run_benchmarks.sh >/dev/null 2>&1; then
    summary="$(find "$bench_dir" -name 'summary.md' | head -1)"
    if [ -n "$summary" ]; then
      # summary.md tiers include julia_cli / sjulia_embedded_cli / sjulia_vm_bytecode
      jc=$(grep -iE 'julia_cli' "$summary" | grep -oE '[0-9]+\.[0-9]+' | head -1)
      se=$(grep -iE 'sjulia_embedded_cli' "$summary" | grep -oE '[0-9]+\.[0-9]+' | head -1)
      sb=$(grep -iE 'sjulia_vm_bytecode' "$summary" | grep -oE '[0-9]+\.[0-9]+' | head -1)
      [ -n "${jc:-}" ] && [ -n "${se:-}" ] && NS4_CLI=$(awk -v a="$se" -v b="$jc" 'BEGIN{printf "%.2f", b ? a/b : 0}')
      [ -n "${jc:-}" ] && [ -n "${sb:-}" ] && NS4_BC=$(awk -v a="$sb" -v b="$jc" 'BEGIN{printf "%.2f", b ? a/b : 0}')
    fi
  fi
  rm -rf "$bench_dir"
  echo "NS-4: cli=${NS4_CLI}x bytecode=${NS4_BC}x" >&2
fi

# ---------------------------------------------------------------- NS-5
NS5_MED=n/a
if ! skip ns5; then
  echo "== NS-5 cold start ==" >&2
  if [ -x target/release/sjulia ]; then
    prog="$(mktemp --suffix=.jl)"; echo 'println("ok")' > "$prog"
    ./target/release/sjulia "$prog" >/dev/null 2>&1  # warm
    times="$(for i in $(seq 12); do
        /usr/bin/time -p ./target/release/sjulia "$prog" 2>&1 >/dev/null | awk '/^real/{print $2}'
      done | sort -n)"
    NS5_MED=$(echo "$times" | awk '{a[NR]=$1} END{print (NR%2)?a[(NR+1)/2]:(a[NR/2]+a[NR/2+1])/2}')
    rm -f "$prog"
  fi
  echo "NS-5: median=${NS5_MED}s" >&2
fi

# ---------------------------------------------------------------- NS-6
NS6_PASS=n/a NS6_FAIL=n/a NS6_DUR=n/a NS6_CONCL=n/a
if ! skip ns6; then
  echo "== NS-6 full nextest (latest main-full CI) ==" >&2
  if command -v gh >/dev/null 2>&1; then
    run_json="$(gh run list --workflow main-full.yml --branch main --limit 1 \
        --json conclusion,createdAt,updatedAt 2>/dev/null || echo '[]')"
    NS6_CONCL=$(echo "$run_json" | grep -oE '"conclusion":"[^"]*"' | head -1 | sed 's/.*://;s/"//g')
    started=$(echo "$run_json" | grep -oE '"createdAt":"[^"]*"' | head -1 | sed 's/.*://;s/"//g')
    ended=$(echo "$run_json" | grep -oE '"updatedAt":"[^"]*"' | head -1 | sed 's/.*://;s/"//g')
    if [ -n "${started:-}" ] && [ -n "${ended:-}" ]; then
      s=$(date -d "$started" +%s 2>/dev/null || echo 0)
      e=$(date -d "$ended" +%s 2>/dev/null || echo 0)
      [ "$s" -gt 0 ] && [ "$e" -gt 0 ] && NS6_DUR=$(awk -v s="$s" -v e="$e" 'BEGIN{printf "%.1f", (e-s)/60}')
    fi
  fi
  echo "NS-6: latest main-full conclusion=${NS6_CONCL} dur=${NS6_DUR}min" >&2
fi

# ---------------------------------------------------------------- NS-7
echo "== NS-7 debt barometer ==" >&2
NS7_WORKAROUNDS=$(grep -cE '^\| W-[0-9]+' docs/vm/WORKAROUNDS.md 2>/dev/null || echo n/a)
NS7_INVENTORY="$(bash scripts/check_structural_debt_inventory.sh 2>&1 \
  | sed -nE 's/^  ([a-z0-9_]+): ([0-9]+).*/\1=\2/p' | tr '\n' ' ')"
echo "NS-7: workarounds=$NS7_WORKAROUNDS" >&2

# NS-7 (c): Instr / TypedLoopOp fused-op variant counts (Issue #10817, wired
# into nightly collection by #10899). Delegates to scripts/loc_report.sh's
# `--variants-only` fast path so this reuses that script's enum-variant
# extraction instead of re-parsing instr.rs/executable.rs a second time here.
# Record-only: per NORTH_STAR.md「動いたら」policy, an increase is NOT a hard
# regression in --check-regression below — fused-op additions can be
# legitimate optimizations (Issue #10817); reviewers instead confirm new
# variants went through the #10814 metadata-derivation precondition.
NS7C_INSTR=n/a NS7C_TYPED_LOOP=n/a
if ! skip ns7c; then
  ns7c_out="$(bash scripts/loc_report.sh --variants-only 2>/dev/null || true)"
  v="$(echo "$ns7c_out" | sed -nE 's/^instr_variants=([0-9]+)$/\1/p')"
  [ -n "$v" ] && NS7C_INSTR="$v"
  v="$(echo "$ns7c_out" | sed -nE 's/^typed_loop_variants=([0-9]+)$/\1/p')"
  [ -n "$v" ] && NS7C_TYPED_LOOP="$v"
fi
echo "NS-7 (c): instr_variants=$NS7C_INSTR typed_loop_variants=$NS7C_TYPED_LOOP" >&2
echo "NS-7 (d): semantic-implementation-lanes-per-construct is manual-only (not machine-measured) — see docs/vm/NORTH_STAR.md NS-7 (d) for the quarterly manual-tally procedure" >&2

# ---------------------------------------------------------------- emit markdown
{
  echo "# North Star レコード — $DATE"
  echo
  echo "自動収集 (\`scripts/north_star_report.sh\`, Issue #8682)。指標定義: \`docs/vm/NORTH_STAR.md\`。"
  echo
  echo "- **計測日**: $DATE"
  echo "- **git commit**: $COMMIT"
  echo "- **julia**: $JULIA_VER"
  echo "- **\`julia/\` サブモジュール**: $CORPUS_COMMIT"
  echo "- **環境**: $ENVIRONMENT (load $LOAD)"
  echo
  echo "| # | 指標 | 値 |"
  echo "|---|------|-----|"
  echo "| NS-1 | パリティ fixture 通過率 | ${NS1_RATE}% (${NS1_PASS}/${NS1_TOTAL}) |"
  echo "| NS-2 | コーパスパース成功率 | ${NS2_RATE}% (${NS2_CLEAN}/${NS2_TOTAL}, panic ${NS2_PANICS}) |"
  echo "| NS-3 | iOS サンプル動作率 | ${NS3_DETAIL} |"
  echo "| NS-4 | 代表ベンチ upstream 比 | CLI ${NS4_CLI}× / bytecode ${NS4_BC}× |"
  echo "| NS-5 | cold 起動時間 | ${NS5_MED} s |"
  echo "| NS-6 | full nextest (latest main-full CI) | ${NS6_CONCL}, ${NS6_DUR} min |"
  echo "| NS-7 | open workaround | ${NS7_WORKAROUNDS} 件 |"
  echo "| NS-7 (c) | Instr / TypedLoopOp variant 数 | Instr ${NS7C_INSTR} / TypedLoopOp ${NS7C_TYPED_LOOP} |"
  echo
  echo "## NS-7 構造負債棚卸し"
  echo
  for kv in $NS7_INVENTORY; do echo "- ${kv%%=*}: ${kv##*=}"; done
  echo
  echo "NS-7 (d) 構文あたりの意味論実装系統数は手動棚卸しのみ(機械計測なし)。"
  echo "四半期レビュー時に \`docs/vm/NORTH_STAR.md\` NS-7 (d) の運用に従い STATUS.md へ記録する。"
  echo
  echo "時間系 (NS-4/5/6) は $ENVIRONMENT 計測。NORTH_STAR.md の規約により CI 計測を正とする。"
} > "$MD"
echo "north_star_report: wrote $MD" >&2

# ---------------------------------------------------------------- emit TSV
if [ ! -f "$TSV" ]; then
  printf 'date\tcommit\tenv\tns1_rate\tns1_pass\tns1_total\tns2_rate\tns2_clean\tns2_total\tns2_panics\tns3_rate\tns3_infra_rate\tns4_cli\tns4_bc\tns5_med\tns6_concl\tns6_dur\tns7_workarounds\tns7c_instr_variants\tns7c_typed_loop_variants\n' > "$TSV"
fi
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
  "$DATE" "$COMMIT" "$ENVIRONMENT" \
  "$NS1_RATE" "$NS1_PASS" "$NS1_TOTAL" \
  "$NS2_RATE" "$NS2_CLEAN" "$NS2_TOTAL" "$NS2_PANICS" \
  "$NS3_RATE" "$NS3_INFRA_RATE" "$NS4_CLI" "$NS4_BC" "$NS5_MED" \
  "$NS6_CONCL" "$NS6_DUR" "$NS7_WORKAROUNDS" "$NS7C_INSTR" "$NS7C_TYPED_LOOP" >> "$TSV"
echo "north_star_report: appended row to $TSV" >&2

# ---------------------------------------------------------------- regression check
REGRESSION=0
if [ "$CHECK_REGRESSION" -eq 1 ]; then
  # previous data row = second-to-last non-header line (the last is what we just wrote)
  prev="$(grep -v '^date' "$TSV" | tail -2 | head -1)"
  curr="$(tail -1 "$TSV")"
  if [ -n "$prev" ] && [ "$prev" != "$curr" ]; then
    # column indices (1-based): 4 ns1_rate, 7 ns2_rate, 9 ns2_total, 13 ns4_cli,
    # 15 ns5_med, 18 ns7_workarounds
    # Columns 19/20 (ns7c_instr_variants / ns7c_typed_loop_variants, Issue
    # #10899) are deliberately NOT compared here: NORTH_STAR.md NS-7 (c)
    # treats fused-op variant growth as record-only (an increase can be a
    # legitimate optimization), unlike the NS-7 (a) workaround count above —
    # see NS-7 (c)「動いたら」in docs/vm/NORTH_STAR.md for the human review
    # trigger instead (new variant via the #10814 metadata-derivation path).
    p1=$(echo "$prev"  | cut -f4);  c1=$(echo "$curr" | cut -f4)
    p2=$(echo "$prev"  | cut -f7);  c2=$(echo "$curr" | cut -f7)
    p2t=$(echo "$prev" | cut -f9);  c2t=$(echo "$curr" | cut -f9)
    p4=$(echo "$prev"  | cut -f13); c4=$(echo "$curr" | cut -f13)
    p5=$(echo "$prev"  | cut -f15); c5=$(echo "$curr" | cut -f15)
    p7=$(echo "$prev"  | cut -f18); c7=$(echo "$curr" | cut -f18)
    worse_num() { awk -v p="$1" -v c="$2" 'BEGIN{print (p!="n/a"&&c!="n/a"&&c<p)?1:0}'; }
    up_num()    { awk -v p="$1" -v c="$2" 'BEGIN{print (p!="n/a"&&c!="n/a"&&c>p)?1:0}'; }
    over_pct()  { awk -v p="$1" -v c="$2" -v f="$3" 'BEGIN{print (p!="n/a"&&c!="n/a"&&p>0&&c>p*(1+f))?1:0}'; }
    [ "$(worse_num "$p1" "$c1")" = 1 ] && { echo "REGRESSION NS-1 parity rate down: $p1% -> $c1%" >&2; REGRESSION=1; }
    if [ "$p2t" = "$c2t" ]; then
      [ "$(worse_num "$p2" "$c2")" = 1 ] && { echo "REGRESSION NS-2 parse rate down (same corpus): $p2% -> $c2%" >&2; REGRESSION=1; }
    fi
    [ "$(up_num "$p7" "$c7")" = 1 ]   && { echo "REGRESSION NS-7 workaround count up: $p7 -> $c7" >&2; REGRESSION=1; }
    [ "$(over_pct "$p4" "$c4" 0.20)" = 1 ] && { echo "REGRESSION NS-4 CLI ratio +>20%: ${p4}x -> ${c4}x" >&2; REGRESSION=1; }
    [ "$(over_pct "$p5" "$c5" 0.20)" = 1 ] && { echo "REGRESSION NS-5 cold start +>20%: ${p5}s -> ${c5}s" >&2; REGRESSION=1; }
    [ "$REGRESSION" -eq 0 ] && echo "north_star_report: no trend regression vs previous record" >&2
  else
    echo "north_star_report: no previous record to compare (regression check skipped)" >&2
  fi
fi

[ "$REGRESSION" -eq 1 ] && exit 3
exit 0
