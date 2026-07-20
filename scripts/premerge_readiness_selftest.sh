#!/usr/bin/env bash
# Negative self-test for the draft -> certified-ready workflow (Issue #11056).
# It runs the real premerge_gate.sh in isolated temporary repositories with a
# fake `gh`, proving ready/wrong-head/gate-failure inputs cannot reach merge.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/sjulia-premerge-readiness.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

FAKE_BIN="$TMP/bin"
mkdir -p "$FAKE_BIN"

cat > "$FAKE_BIN/gh" <<'GH'
#!/bin/bash
set -euo pipefail
printf '%s\n' "$*" >> "$FAKE_GH_LOG"

case "$1 $2" in
  "pr view")
    if printf '%s\n' "$*" | grep -q -- '--json state,isDraft,headRefOid,baseRefName,baseRefOid'; then
      if [ "${FAKE_GH_FAIL_POST_READY_VIEW:-0}" = "1" ] && [ "$(cat "$FAKE_GH_DRAFT")" = "false" ]; then
        exit 5
      fi
      printf '%s\t%s\t%s\t%s\t%s\n' "$(cat "$FAKE_GH_STATE")" "$(cat "$FAKE_GH_DRAFT")" \
        "$FAKE_GH_HEAD" "$FAKE_GH_BASE" "$(git --git-dir="$FAKE_ORIGIN" rev-parse refs/heads/main)"
    elif printf '%s\n' "$*" | grep -q -- '--json state'; then
      printf '%s\n' "$(cat "$FAKE_GH_STATE")"
    else
      exit 2
    fi
    ;;
  "pr ready")
    if [ "${4:-}" = "--undo" ]; then
      printf 'true\n' > "$FAKE_GH_DRAFT"
    else
      printf 'false\n' > "$FAKE_GH_DRAFT"
      if [ "${FAKE_GH_ADVANCE_BASE_AFTER_READY:-0}" = "1" ]; then
        git --git-dir="$FAKE_ORIGIN" update-ref refs/heads/main "$FAKE_ADVANCE_SHA"
      fi
    fi
    ;;
  "pr merge")
    [ "$(cat "$FAKE_GH_DRAFT")" = "false" ] || {
      echo "merge attempted while draft" >&2
      exit 3
    }
    [ "$(tail -1 "$FAKE_GH_STATUS")" = "success" ] || {
      echo "merge attempted without successful certification status" >&2
      exit 9
    }
    printf '%s\n' "$*" | grep -q -- "--match-head-commit $FAKE_GH_HEAD" || {
      echo "merge was not pinned to expected head" >&2
      exit 4
    }
    [ "${FAKE_GH_FAIL_MERGE:-0}" != "1" ] || exit 6
    if [ "${FAKE_GH_KEEP_OPEN_AFTER_MERGE:-0}" != "1" ]; then
      printf 'MERGED\n' > "$FAKE_GH_STATE"
    fi
    ;;
  "api --method")
    state=""
    context=""
    while [ "$#" -gt 0 ]; do
      case "$1" in
        state=*) state="${1#state=}" ;;
        context=*) context="${1#context=}" ;;
      esac
      shift
    done
    [ "$context" = "sjulia/guarded-certification" ] || exit 7
    [ -n "$state" ] || exit 8
    printf '%s\n' "$state" >> "$FAKE_GH_STATUS"
    ;;
  *) exit 2 ;;
esac
GH
chmod +x "$FAKE_BIN/gh"

cat > "$FAKE_BIN/bash" <<'BASH'
#!/bin/bash
set -euo pipefail
if [ "${1:-}" = "-c" ]; then
  printf '%s\n' "$2" >> "$FAKE_GATE_LOG"
  if [ "$2" = "touch readiness-selftest-dirty" ]; then
    touch readiness-selftest-dirty
    exit 0
  fi
  [ "$2" != "false" ]
  exit
fi
exec /bin/bash "$@"
BASH
chmod +x "$FAKE_BIN/bash"

new_repo() {
  local name="$1"
  local bare="$TMP/$name-origin.git"
  local repo="$TMP/$name"

  git init --bare --quiet "$bare"
  git init --quiet -b main "$repo"
  git -C "$repo" config user.name selftest
  git -C "$repo" config user.email selftest@example.invalid
  printf 'base\n' > "$repo/state.txt"
  git -C "$repo" add state.txt
  git -C "$repo" commit --quiet -m base
  git -C "$repo" remote add origin "$bare"
  git -C "$repo" push --quiet -u origin main
  git -C "$repo" switch --quiet -c feature
  printf 'feature\n' >> "$repo/state.txt"
  git -C "$repo" commit --quiet -am feature
  git -C "$repo" switch --quiet main
  printf 'advance\n' >> "$repo/state.txt"
  git -C "$repo" commit --quiet -am advance
  git -C "$repo" push --quiet origin HEAD:refs/heads/advance-candidate
  git -C "$repo" switch --quiet feature
  printf '%s\n' "$repo"
}

run_case() {
  local name="$1"
  local draft="$2"
  local head_mode="$3"
  local gate_cmd="$4"
  local expected="$5"
  local expected_reason="$6"
  local extra_option="${7:-}"
  local behavior="${8:-}"
  local repo head rc
  local origin advance_sha
  local args

  repo="$(new_repo "$name")"
  head="$(git -C "$repo" rev-parse HEAD)"
  origin="$TMP/$name-origin.git"
  advance_sha="$(git --git-dir="$origin" rev-parse refs/heads/advance-candidate)"
  : > "$TMP/$name.log"
  : > "$TMP/$name.gates"
  : > "$TMP/$name.statuses"
  printf '%s\n' "$draft" > "$TMP/$name.draft"
  printf 'OPEN\n' > "$TMP/$name.state"

  if [ "$head_mode" = "wrong" ]; then
    head="$(git -C "$repo" rev-parse HEAD^)"
  fi

  set +e
  (
    cd "$repo"
    args=(--pr 123 --gate-cmd "$gate_cmd")
    if [ -n "$extra_option" ]; then
      args+=("$extra_option")
    fi
    PATH="$FAKE_BIN:$PATH" \
    FAKE_GH_LOG="$TMP/$name.log" \
    FAKE_GH_DRAFT="$TMP/$name.draft" \
    FAKE_GH_HEAD="$head" \
    FAKE_GH_BASE="main" \
    FAKE_GH_STATE="$TMP/$name.state" \
    FAKE_ORIGIN="$origin" \
    FAKE_ADVANCE_SHA="$advance_sha" \
    FAKE_GATE_LOG="$TMP/$name.gates" \
    FAKE_GH_STATUS="$TMP/$name.statuses" \
    SJULIA_GITHUB_REPOSITORY="AtelierArith/ailujsoi" \
    FAKE_GH_FAIL_POST_READY_VIEW="$([ "$behavior" = post_ready_view_fail ] && printf 1 || printf 0)" \
    FAKE_GH_ADVANCE_BASE_AFTER_READY="$([ "$behavior" = base_advance_after_ready ] && printf 1 || printf 0)" \
    FAKE_GH_FAIL_MERGE="$([ "$behavior" = merge_fail ] && printf 1 || printf 0)" \
    FAKE_GH_KEEP_OPEN_AFTER_MERGE="$([ "$behavior" = merge_success_open ] && printf 1 || printf 0)" \
      /bin/bash "$ROOT/scripts/premerge_gate.sh" "${args[@]}"
  ) > "$TMP/$name.out" 2>&1
  rc=$?
  set -e

  if [ "$expected" = "pass" ]; then
    [ "$rc" -eq 0 ] || {
      cat "$TMP/$name.out" >&2
      echo "FAIL: $name should pass" >&2
      exit 1
    }
    grep -q '^pr ready 123$' "$TMP/$name.log"
    grep -q '^pr merge 123 --merge --match-head-commit ' "$TMP/$name.log"
    grep -q '^bash scripts/run_source_only_audits.sh$' "$TMP/$name.gates"
    grep -q '^bash scripts/check_source_only_audit_sync.sh$' "$TMP/$name.gates"
    grep -q '^timeout 1800 cargo clippy --all-targets -- -D warnings$' "$TMP/$name.gates"
    grep -q '^true$' "$TMP/$name.gates"
    [ "$(tr '\n' ' ' < "$TMP/$name.statuses")" = "pending success " ] || {
      echo "FAIL: $name did not publish pending -> success status lifecycle" >&2
      exit 1
    }
    [ "$(cat "$TMP/$name.state")" = "MERGED" ]
  else
    [ "$rc" -ne 0 ] || {
      echo "FAIL: $name should fail" >&2
      exit 1
    }
    grep -q "$expected_reason" "$TMP/$name.out" || {
      cat "$TMP/$name.out" >&2
      echo "FAIL: $name did not report expected reason: $expected_reason" >&2
      exit 1
    }
    if [ "$behavior" != "merge_fail" ] && [ "$behavior" != "merge_success_open" ] && \
        grep -q '^pr merge ' "$TMP/$name.log"; then
      echo "FAIL: $name reached merge" >&2
      exit 1
    fi
  fi

  case "$behavior" in
    post_ready_view_fail|base_advance_after_ready|merge_fail|merge_success_open)
      [ "$(cat "$TMP/$name.draft")" = "true" ] || {
        echo "FAIL: $name was not returned to draft" >&2
        exit 1
      }
      [ "$(tr '\n' ' ' < "$TMP/$name.statuses")" = "pending success failure " ] || {
        echo "FAIL: $name did not revoke its successful certification" >&2
        exit 1
      }
      ;;
  esac

  printf '[premerge_readiness_selftest] PASS: %s\n' "$name"
}

run_case ready_is_rejected false exact true fail 'already ready'
run_case wrong_head_is_rejected true wrong true fail 'different head is uncertified'
run_case check_only_cannot_certify true exact true fail 'cannot be combined with --pr' --check-only
run_case failed_gate_stays_draft true exact false fail 'gate failed'
[ "$(tr '\n' ' ' < "$TMP/failed_gate_stays_draft.statuses")" = "pending failure " ] || {
  echo "FAIL: failed gate did not revoke its pending certification" >&2
  exit 1
}
[ "$(cat "$TMP/failed_gate_stays_draft.draft")" = "true" ] || {
  echo "FAIL: failed gate changed draft state" >&2
  exit 1
}
run_case mutating_gate_is_rejected true exact 'touch readiness-selftest-dirty' fail 'became dirty'
run_case post_ready_view_failure_redrafts true exact true fail 'cannot inspect PR' '' post_ready_view_fail
run_case base_advance_after_ready_redrafts true exact true fail 'base advanced' '' base_advance_after_ready
run_case merge_failure_redrafts true exact true fail 'merge failed' '' merge_fail
run_case merge_success_without_merged_state_redrafts true exact true fail 'not MERGED' '' merge_success_open
run_case certified_draft_transitions_and_merges true exact true pass ''

echo '[premerge_readiness_selftest] OK: uncertified workflows cannot reach ready/merge.'
