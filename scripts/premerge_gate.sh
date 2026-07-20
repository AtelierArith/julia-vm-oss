#!/usr/bin/env bash
# premerge_gate.sh
#
# Guarded final-verification sequence for lead merges (Issue #9644).
#
# GitHub Actions is disabled on this repo, so the ONLY thing standing between
# a clippy warning and `main` is the lead rerunning the local gates on the
# EXACT current `origin/main`. Issue #9641 landed a
# `clippy::collapsible_match` warning on main precisely because the final
# `cargo clippy --all-targets -- -D warnings` gate was not rerun after `main`
# advanced during a parallel merge window. This script makes that sequence
# mechanical:
#
#   1. Freshness pre-check : fetch origin/main, record its SHA, and require
#      that HEAD already CONTAINS that exact SHA (fail otherwise; with
#      --merge-main, attempt `git merge --no-edit origin/main` first).
#   2. Clean-tree check    : refuse to certify a dirty working tree.
#   3. Gates               : by default, `scripts/run_source_only_audits.sh`
#      (Issue #10870) — which runs every `premerge_default=true` row of the
#      canonical registry `scripts/source_only_audits.tsv`: the sub-second
#      snapshot/ratchet audits `scripts/check_no_new_domain_builtins.sh`
#      (Issue #9696), `scripts/audit_base_cache_schema_fingerprint.sh`
#      (Issue #10256), `scripts/check_structural_debt_inventory.sh`, and
#      `scripts/check_panic_free_ratchet.sh` (the latter two were CI-enforced
#      but NOT wired into this gate before #10870, which is exactly how they
#      drifted red on main without any guarded merge catching it — the same
#      failure class as #8740/#9920-#9925). Actions is disabled, so these
#      local-only audits must run in the PR that causes drift, not be
#      discovered red on main later. Also runs
#      `scripts/check_source_only_audit_sync.sh` as its OWN gate line (not
#      routed through the runner above), so a future change that removes the
#      runner line is itself caught — the registry/premerge wiring guards
#      itself. Followed by `timeout 1800 cargo clippy
#      --all-targets -- -D warnings`; add nextest via --nextest/--full-suite,
#      or add custom PR gates with repeated --gate-cmd (without --pr, custom
#      commands retain the legacy replace-default behavior). Run
#      `bash scripts/premerge_gate.sh --list-gates` for a dry run that prints
#      the exact gate command list without fetching, checking the tree, or
#      executing anything (Issue #10870).
#   4. Freshness re-check  : fetch origin/main AGAIN after the gates. If the
#      remote moved during the verification window, FAIL — the verification
#      is stale; merge the new origin/main and rerun. This is the
#      negative/diagnostic mode Issue #9644 asks for.
#   5. With --pr, require that the PR is still OPEN + draft and targets this
#      base/head both before and after the gates. Publish the exact-head
#      `sjulia/guarded-certification` status required by the strict GitHub
#      ruleset, then mark it ready and merge it with `--match-head-commit`.
#      Failures revoke the status and return the PR to draft unless GitHub
#      reports that it actually merged (Issues #11056/#11087).
#
# NAMING: deliberately NOT named `check_*.sh` so it does NOT trip the
# "all check_*.sh are registered in ci.yml/docs" audit (same convention as
# `scripts/branch_guard.sh`). This is a developer-side lead-merge guard, not
# a CI audit gate.
#
# Usage:
#   bash scripts/premerge_gate.sh                       # freshness + clippy + recheck
#   bash scripts/premerge_gate.sh --list-gates          # dry run: print the gate list, do nothing else
#   bash scripts/premerge_gate.sh --check-only          # freshness checks only (no gates)
#   bash scripts/premerge_gate.sh --merge-main          # merge origin/main first if behind
#   bash scripts/premerge_gate.sh --nextest 'fixture_tests promotion::'
#   bash scripts/premerge_gate.sh --full-suite          # + full release nextest + exception parity
#   bash scripts/premerge_gate.sh --metamorphic         # force metamorphic equivalence lanes (#10465)
#   bash scripts/premerge_gate.sh --pr 1234             # certify draft PR, mark ready, merge pinned HEAD
#   bash scripts/premerge_gate.sh --gate-cmd 'bash scripts/test_aot.sh'   # append for --pr; replace otherwise
#
# Exit codes: 0 = certified green on current origin/main; non-zero otherwise.

set -euo pipefail

# Gate context marker (Issue #10946): upstream-corpus-dependent tests
# (parser_corpus_base_ratchet, base_exports_do_not_exceed_upstream) skip
# gracefully when the julia/ submodule is absent — acceptable ad hoc, but a
# SILENT pass inside a certification run false-greened a full-suite gate from
# a submodule-less worktree (incident #10935). With this set, those tests
# FAIL when the corpus is missing, so a gate run either compares the corpus
# or refuses to certify.
export SJULIA_REQUIRE_CORPUS=1

REMOTE="origin"
BASE_BRANCH="main"
CHECK_ONLY=0
MERGE_MAIN=0
FULL_SUITE=0
METAMORPHIC=0
LIST_GATES=0
PR_NUMBER=""
CERTIFICATION_CONTEXT="${SJULIA_CERTIFICATION_CONTEXT:-sjulia/guarded-certification}"
GITHUB_REPOSITORY="${SJULIA_GITHUB_REPOSITORY:-}"
NEXTEST_FILTERS=()
GATE_CMDS=()
EXTRA_GATE_CMDS=()
PR_READY_BY_GATE=0
CERTIFICATION_STATUS_ACTIVE=0

usage() {
  sed -n '2,52p' "$0" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
  case "$1" in
    --check-only) CHECK_ONLY=1 ;;
    --merge-main) MERGE_MAIN=1 ;;
    --full-suite) FULL_SUITE=1 ;;
    --metamorphic) METAMORPHIC=1 ;;
    --list-gates) LIST_GATES=1 ;;
    --nextest)
      shift
      [ $# -gt 0 ] || { echo "ERROR: --nextest requires an argument" >&2; exit 2; }
      NEXTEST_FILTERS+=("$1")
      ;;
    --gate-cmd)
      shift
      [ $# -gt 0 ] || { echo "ERROR: --gate-cmd requires an argument" >&2; exit 2; }
      [ -n "$1" ] || { echo "ERROR: --gate-cmd cannot be empty" >&2; exit 2; }
      EXTRA_GATE_CMDS+=("$1")
      ;;
    --pr)
      shift
      [ $# -gt 0 ] || { echo "ERROR: --pr requires a PR number" >&2; exit 2; }
      PR_NUMBER="$1"
      ;;
    --remote)
      shift
      [ $# -gt 0 ] || { echo "ERROR: --remote requires an argument" >&2; exit 2; }
      REMOTE="$1"
      ;;
    --base)
      shift
      [ $# -gt 0 ] || { echo "ERROR: --base requires an argument" >&2; exit 2; }
      BASE_BRANCH="$1"
      ;;
    -h|--help) usage; exit 0 ;;
    *) echo "ERROR: unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
  shift
done

BASE_REF="$REMOTE/$BASE_BRANCH"

say()  { printf '[premerge_gate] %s\n' "$*"; }
fail() { printf '[premerge_gate] FAIL: %s\n' "$*" >&2; exit 1; }

if [ -n "$PR_NUMBER" ] && [ "$CHECK_ONLY" -eq 1 ]; then
  fail "--check-only cannot be combined with --pr. Readiness requires an executed gate set (Issue #11056)."
fi

rollback_ready_on_exit() {
  local rc="$?"
  local state
  trap - EXIT

  if [ "$rc" -ne 0 ] && [ "$CERTIFICATION_STATUS_ACTIVE" -eq 1 ]; then
    say "revoking certification status for $HEAD_SHA ..."
    if ! publish_certification_status failure "Guarded certification failed or was invalidated"; then
      printf '[premerge_gate] FAIL: could not publish failure status for %s. Treat the head as uncertified.\n' \
        "$HEAD_SHA" >&2
    fi
  fi

  if [ "$rc" -ne 0 ] && [ "$PR_READY_BY_GATE" -eq 1 ]; then
    state="$(gh pr view "$PR_NUMBER" --json state --jq '.state' 2>/dev/null || printf 'UNKNOWN')"
    if [ "$state" != "MERGED" ]; then
      say "aborted after readiness; returning PR #$PR_NUMBER to draft ..."
      if ! gh pr ready "$PR_NUMBER" --undo; then
        printf '[premerge_gate] FAIL: PR #%s could not be returned to draft (state: %s). Return it manually before continuing.\n' \
          "$PR_NUMBER" "$state" >&2
      fi
    fi
  fi
  exit "$rc"
}
trap rollback_ready_on_exit EXIT

publish_certification_status() {
  local state="$1"
  local description="$2"

  [ -n "$GITHUB_REPOSITORY" ] || {
    GITHUB_REPOSITORY="$(gh repo view --json nameWithOwner --jq '.nameWithOwner')" || return 1
  }
  gh api --method POST "repos/$GITHUB_REPOSITORY/statuses/$HEAD_SHA" \
    -f "state=$state" \
    -f "context=$CERTIFICATION_CONTEXT" \
    -f "description=$description" >/dev/null
}

# require_pr_state — prove that --pr names the OPEN PR for this exact local
# HEAD and base, and that its draft state is the one expected at this phase.
# The check runs both before the gates and immediately before readiness so a
# concurrent push, retarget, manual ready action, or close invalidates the run.
require_pr_state() {
  local expected_draft="$1"
  local phase="$2"
  local metadata state is_draft head_oid base_name base_oid

  if ! metadata="$(gh pr view "$PR_NUMBER" \
      --json state,isDraft,headRefOid,baseRefName,baseRefOid \
      --jq '[.state, (.isDraft|tostring), .headRefOid, .baseRefName, .baseRefOid] | @tsv')"; then
    fail "cannot inspect PR #$PR_NUMBER during $phase; readiness is not certified."
  fi
  IFS=$'\t' read -r state is_draft head_oid base_name base_oid <<< "$metadata"

  [ "$state" = "OPEN" ] || fail "PR #$PR_NUMBER is $state during $phase; expected OPEN."
  [ "$is_draft" = "$expected_draft" ] || {
    if [ "$expected_draft" = "true" ]; then
      fail "PR #$PR_NUMBER is already ready during $phase. Uncertified PRs must remain draft; \
return it to draft with 'gh pr ready $PR_NUMBER --undo', then rerun the guarded gate (Issue #11056)."
    fi
    fail "PR #$PR_NUMBER is still draft after the guarded readiness transition."
  }
  [ "$head_oid" = "$HEAD_SHA" ] || fail "PR #$PR_NUMBER head is $head_oid, but this gate is \
running at $HEAD_SHA. Fetch/check out the exact PR head and rerun; a different head is uncertified."
  [ "$base_name" = "$BASE_BRANCH" ] || fail "PR #$PR_NUMBER targets $base_name, but this gate \
certifies $BASE_BRANCH. Retarget it or pass the matching --base explicitly."
  [ "$base_oid" = "$MAIN_SHA" ] || fail "PR #$PR_NUMBER currently sees base $base_oid, but the \
gates certified $MAIN_SHA. The base advanced or the PR metadata is stale; return to draft, merge current \
$BASE_REF, and rerun the full gate."
}

require_certified_local_state() {
  local phase="$1"
  local current_head
  current_head="$(git rev-parse HEAD)"
  [ "$current_head" = "$HEAD_SHA" ] || fail "local HEAD changed during $phase \
($HEAD_SHA -> $current_head). The gates no longer certify the PR head."
  [ -z "$(git status --porcelain)" ] || fail "working tree became dirty during $phase. \
The gates certify committed HEAD only; inspect the mutation and rerun from a clean tree."
}

# build_gate_cmds — populate GATE_CMDS with the gate command list that will
# run in step 3, honoring any --gate-cmd overrides collected during arg
# parsing. Factored into a function so --list-gates (a pure dry run) and the
# real step-3 execution share exactly one definition of "the default gate
# set" (Issue #10870) — no second hand-copied list to drift out of sync.
build_gate_cmds() {
  if [ "${#GATE_CMDS[@]}" -eq 0 ]; then
    # A merge-capable --pr invocation always includes the default source audits
    # and clippy. Custom commands are additional gates, never a replacement;
    # otherwise `--gate-cmd true` could silently certify nothing (#11056).
    if [ -z "$PR_NUMBER" ] && [ "${#EXTRA_GATE_CMDS[@]}" -gt 0 ]; then
      GATE_CMDS=("${EXTRA_GATE_CMDS[@]}")
      return
    fi
    # Source-only audit gate set (Issue #10870): reads
    # scripts/source_only_audits.tsv (the canonical registry) and runs every
    # premerge_default=true row — currently
    # check_no_new_domain_builtins.sh (Issue #9696),
    # audit_base_cache_schema_fingerprint.sh (Issue #10256),
    # check_structural_debt_inventory.sh, and check_panic_free_ratchet.sh.
    # Adding a new fast source-only audit to this gate means adding a
    # registry row, not editing this script.
    GATE_CMDS+=("bash scripts/run_source_only_audits.sh")
    # Registry <-> premerge/CI drift check (Issue #10870): a SEPARATE gate
    # line, not routed through run_source_only_audits.sh, so it still fires
    # even if a future change removes the line above — the exact "audit
    # registered but nothing local runs it" failure mode this whole PR
    # exists to close. Read-only; never edits .github/workflows/*.
    GATE_CMDS+=("bash scripts/check_source_only_audit_sync.sh")
    # Issue #11274: execute only the semantic negative controls whose mutation
    # targets changed in this PR. The full suite remains an explicit audit
    # authoring gate; this bounded path catches stale injector anchors during
    # ordinary source refactors without adding its several-minute cost.
    GATE_CMDS+=("bash scripts/check_audit_negative_selftest.sh --changed-from $BASE_REF")
    # Issue #11253: keep the local reference-toolchain command owned by the
    # canonical executable lane registry.
    GATE_CMDS+=("timeout 1800 bash scripts/run_clippy_lanes.sh default")
    for filter in ${NEXTEST_FILTERS[@]+"${NEXTEST_FILTERS[@]}"}; do
      GATE_CMDS+=("timeout 1800 cargo nextest run --release --test $filter")
    done
    if [ "$FULL_SUITE" -eq 1 ]; then
      GATE_CMDS+=("timeout 1800 cargo nextest run --release --no-fail-fast")
      # Issue #10813 / #11148: the full semantic gate also executes the
      # upstream-vs-sjulia exception type/catchability corpus. The lane owns
      # its REPL-feature release build and a two-sided, issue-linked allowlist,
      # so a new divergence or a stale resolved row both fail certification.
      GATE_CMDS+=("bash scripts/exception_parity_ratchet.sh")
    fi
    # Metamorphic equivalence lanes (Issues #10465/#10452): bounded differential
    # gate over a curated corpus, NOT the fixture Cartesian product. It is
    # automatic for semantic-pipeline changes and can also be forced explicitly
    # with --metamorphic. Builds the needed release sjulia/juliars binaries.
    if [ "$METAMORPHIC" -eq 1 ] || metamorphic_paths_changed; then
      GATE_CMDS+=("bash scripts/metamorphic_equivalence.sh")
    fi
    for cmd in ${EXTRA_GATE_CMDS[@]+"${EXTRA_GATE_CMDS[@]}"}; do
      GATE_CMDS+=("$cmd")
    done
  fi
}

# metamorphic_paths_changed — require the equivalence matrix whenever the PR
# touches a layer capable of making semantically-equivalent execution lanes
# disagree. SJULIA_PREMERGE_CHANGED_PATHS is a newline-separated test hook for
# the pure --list-gates dry run; production runs derive paths from BASE_REF.
metamorphic_paths_changed() {
  local paths path
  if [ -n "${SJULIA_PREMERGE_CHANGED_PATHS:-}" ]; then
    paths="$SJULIA_PREMERGE_CHANGED_PATHS"
  else
    paths="$(git diff --name-only "$BASE_REF"...HEAD 2>/dev/null || true)"
  fi
  while IFS= read -r path; do
    case "$path" in
      subset_julia_vm/src/*|subset_julia_vm_lowering/*|subset_julia_vm_compile/*|subset_julia_vm_vm/*|subset_julia_vm_parser/*|subset_julia_vm_types/*|subset_julia_vm_bytecode/*|subset_julia_vm_runtime/*|tests/equivalence/*|scripts/metamorphic_equivalence.sh)
        return 0
        ;;
    esac
  done <<EOF
$paths
EOF
  return 1
}

if [ "$LIST_GATES" -eq 1 ]; then
  # Pure dry run (Issue #10870): no git fetch, no clean-tree check, nothing
  # executed — just prove which commands the default (or --gate-cmd
  # overridden) gate set would run. Useful both for humans auditing the
  # guarded merge gate and for a self-test that checks "the registered
  # ratchets are part of the default invocation" without paying for a
  # network fetch or a 30-minute clippy run.
  build_gate_cmds
  say "gate list (--list-gates dry run; nothing executed):"
  for cmd in "${GATE_CMDS[@]}"; do
    printf '  %s\n' "$cmd"
  done
  exit 0
fi

# --- 1. Freshness pre-check -------------------------------------------------
say "fetching $BASE_REF ..."
git fetch --quiet "$REMOTE" "$BASE_BRANCH"
MAIN_SHA="$(git rev-parse "$BASE_REF")"
say "verification window opens at $BASE_REF = $MAIN_SHA"

if ! git merge-base --is-ancestor "$MAIN_SHA" HEAD; then
  if [ "$MERGE_MAIN" -eq 1 ]; then
    say "HEAD is behind $BASE_REF; merging it in (--merge-main) ..."
    if ! git merge --no-edit "$BASE_REF"; then
      fail "merge of $BASE_REF has conflicts. Resolve them (union-resolution \
rules: sjulia-lead-review-merge skill), commit, then rerun this gate."
    fi
  else
    fail "HEAD does not contain the current $BASE_REF ($MAIN_SHA). \
$BASE_BRANCH advanced since this branch was last verified — any clippy/test \
result from the old base is stale (Issue #9641/#9644). Run \
'git merge $BASE_REF' (or rerun with --merge-main), then rerun this gate."
  fi
fi

# --- 2. Clean-tree check ----------------------------------------------------
if [ -n "$(git status --porcelain)" ]; then
  fail "working tree is dirty. Gates certify the committed HEAD only; commit \
or drop the uncommitted changes first (never 'git stash' in this repo)."
fi

HEAD_SHA="$(git rev-parse HEAD)"
say "HEAD = $HEAD_SHA (contains $BASE_REF)"

if [ -n "$PR_NUMBER" ]; then
  say "checking that PR #$PR_NUMBER is the draft for this exact base/head ..."
  require_pr_state "true" "pre-gate check"
  say "publishing pending certification status for $HEAD_SHA ..."
  publish_certification_status pending "Guarded local certification is running" || \
    fail "could not publish pending certification status for $HEAD_SHA."
  CERTIFICATION_STATUS_ACTIVE=1
fi

# --- 3. Gates ----------------------------------------------------------------
if [ "$CHECK_ONLY" -eq 1 ]; then
  say "--check-only: skipping gates."
else
  build_gate_cmds
  for cmd in "${GATE_CMDS[@]}"; do
    say "gate: $cmd"
    bash -c "$cmd" || fail "gate failed: $cmd"
  done
fi
require_certified_local_state "gate execution"

# --- 4. Freshness re-check (the merge-window guard) --------------------------
say "re-fetching $BASE_REF to close the verification window ..."
git fetch --quiet "$REMOTE" "$BASE_BRANCH"
MAIN_SHA_AFTER="$(git rev-parse "$BASE_REF")"
if [ "$MAIN_SHA_AFTER" != "$MAIN_SHA" ]; then
  fail "$BASE_REF advanced DURING the verification window \
($MAIN_SHA -> $MAIN_SHA_AFTER). The gates above certified a base that is no \
longer $BASE_BRANCH; merge the new $BASE_REF into this branch and rerun the \
gate (Issue #9644). Do NOT merge the PR on the strength of this run."
fi

# --- 5. Certify --------------------------------------------------------------
say "OK: gates green on HEAD $HEAD_SHA, which contains the current $BASE_REF ($MAIN_SHA)."
if [ -n "$PR_NUMBER" ]; then
  require_pr_state "true" "post-gate check"
  say "publishing successful certification status for $HEAD_SHA ..."
  publish_certification_status success "Guarded certification passed on current main" || \
    fail "could not publish successful certification status for $HEAD_SHA."
  say "marking certified PR #$PR_NUMBER ready ..."
  PR_READY_BY_GATE=1
  gh pr ready "$PR_NUMBER" || fail "could not mark certified PR #$PR_NUMBER ready."
  require_pr_state "false" "post-transition check"
  require_certified_local_state "readiness transition"

  # GitHub's merge API pins the head but exposes no expected-base argument.
  # Re-fetch at the last possible point and fail back to draft if main moved
  # after the earlier verification-window check.
  say "final re-fetch of $BASE_REF immediately before merge ..."
  git fetch --quiet "$REMOTE" "$BASE_BRANCH"
  FINAL_MAIN_SHA="$(git rev-parse "$BASE_REF")"
  [ "$FINAL_MAIN_SHA" = "$MAIN_SHA" ] || fail "$BASE_REF advanced after readiness \
($MAIN_SHA -> $FINAL_MAIN_SHA). The exit handler will return PR #$PR_NUMBER to draft."
  require_pr_state "false" "final pre-merge check"

  say "merging PR #$PR_NUMBER pinned to the certified head ..."
  if ! gh pr merge "$PR_NUMBER" --merge --match-head-commit "$HEAD_SHA"; then
    PR_STATE="$(gh pr view "$PR_NUMBER" --json state --jq '.state' 2>/dev/null || printf 'UNKNOWN')"
    if [ "$PR_STATE" = "MERGED" ]; then
      say "merge command returned non-zero, but GitHub reports PR #$PR_NUMBER MERGED."
      PR_READY_BY_GATE=0
      CERTIFICATION_STATUS_ACTIVE=0
    else
      fail "merge failed; the exit handler will return PR #$PR_NUMBER to draft. Resolve the cause and rerun the full guarded gate."
    fi
  else
    PR_STATE="$(gh pr view "$PR_NUMBER" --json state --jq '.state' 2>/dev/null || printf 'UNKNOWN')"
    if [ "$PR_STATE" != "MERGED" ]; then
      fail "merge command returned success but PR #$PR_NUMBER is $PR_STATE, not MERGED. \
The exit handler will return it to draft; readiness alone is not completion."
    fi
    PR_READY_BY_GATE=0
    CERTIFICATION_STATUS_ACTIVE=0
  fi
else
  say "certification did not change PR readiness. Push this exact commit to a draft PR, then rerun:"
  say "  bash scripts/premerge_gate.sh [same gate options] --pr <N>"
fi
