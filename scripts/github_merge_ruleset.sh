#!/usr/bin/env bash
# Server-side companion to premerge_gate.sh (Issue #11087).

set -euo pipefail

RULESET_NAME="${SJULIA_MAIN_RULESET_NAME:-protect main}"
CERTIFICATION_CONTEXT="${SJULIA_CERTIFICATION_CONTEXT:-sjulia/guarded-certification}"
GITHUB_REPOSITORY="${SJULIA_GITHUB_REPOSITORY:-}"

usage() {
  echo "usage: $0 --check | --apply | --selftest" >&2
  exit 2
}

validate_ruleset_json() {
  jq -e --arg context "$CERTIFICATION_CONTEXT" '
    .target == "branch" and
    .enforcement == "active" and
    (.bypass_actors | length) == 0 and
    any(.conditions.ref_name.include[]; . == "~DEFAULT_BRANCH") and
    (.conditions.ref_name.exclude | length) == 0 and
    any(.rules[];
      .type == "required_status_checks" and
      .parameters.strict_required_status_checks_policy == true and
      any(.parameters.required_status_checks[];
        .context == $context and (.integration_id == null))
    )
  ' >/dev/null
}

build_payload() {
  jq --arg context "$CERTIFICATION_CONTEXT" '
    (.rules | map(select(.type == "required_status_checks")) | first) as $existing_status_rule |
    {
      name,
      target,
      enforcement,
      bypass_actors,
      conditions,
      rules: (
        [.rules[] | select(.type != "required_status_checks")] +
        [{
          type: "required_status_checks",
          parameters: {
            required_status_checks: (
              ([($existing_status_rule.parameters.required_status_checks // [])[] |
                select(.context != $context)] + [{context: $context}]) |
              unique_by(.context, (.integration_id // -1))
            ),
            strict_required_status_checks_policy: true,
            do_not_enforce_on_create:
              ($existing_status_rule.parameters.do_not_enforce_on_create // true)
          }
        }]
      )
    }
  '
}

selftest() {
  local good bad transformed
  good='{"target":"branch","enforcement":"active","bypass_actors":[],"conditions":{"ref_name":{"include":["~DEFAULT_BRANCH"],"exclude":[]}},"rules":[{"type":"deletion"},{"type":"required_status_checks","parameters":{"strict_required_status_checks_policy":true,"required_status_checks":[{"context":"existing/check","integration_id":42},{"context":"sjulia/guarded-certification"}]}}]}'
  bad='{"target":"branch","enforcement":"active","bypass_actors":[],"conditions":{"ref_name":{"include":["~DEFAULT_BRANCH"],"exclude":[]}},"rules":[{"type":"deletion"},{"type":"required_status_checks","parameters":{"strict_required_status_checks_policy":false,"required_status_checks":[{"context":"sjulia/guarded-certification"}]}}]}'
  printf '%s\n' "$good" | validate_ruleset_json
  if printf '%s\n' "$bad" | validate_ruleset_json; then
    echo "FAIL: non-strict required status was accepted" >&2
    exit 1
  fi
  if printf '%s\n' "$good" | jq '.bypass_actors = [{"actor_id": 1}]' | validate_ruleset_json; then
    echo "FAIL: bypass actor was accepted" >&2
    exit 1
  fi
  if printf '%s\n' "$good" | jq '.conditions.ref_name.exclude = ["~DEFAULT_BRANCH"]' | validate_ruleset_json; then
    echo "FAIL: default-branch exclusion was accepted" >&2
    exit 1
  fi
  if printf '%s\n' "$good" | jq '
      (.rules[] | select(.type == "required_status_checks") |
       .parameters.required_status_checks[] |
       select(.context == "sjulia/guarded-certification")).integration_id = 42
    ' | validate_ruleset_json; then
    echo "FAIL: App-bound certification was accepted without an App publisher" >&2
    exit 1
  fi
  transformed="$(printf '%s\n' "$good" | build_payload)"
  printf '%s\n' "$transformed" | jq -e '
    any(.rules[] | select(.type == "required_status_checks") |
      .parameters.required_status_checks[];
      .context == "existing/check" and .integration_id == 42)
  ' >/dev/null
  printf '%s\n' "$transformed" | jq -e '
    [.rules[] | select(.type == "required_status_checks") |
      .parameters.required_status_checks[] |
      select(.context == "sjulia/guarded-certification" and .integration_id == null)] |
    length == 1
  ' >/dev/null
  echo "[github_merge_ruleset] selftest OK: strict scoped certification preserves existing checks."
}

[ $# -eq 1 ] || usage
case "$1" in
  --selftest)
    selftest
    exit 0
    ;;
  --check|--apply) ;;
  *) usage ;;
esac

[ -n "$GITHUB_REPOSITORY" ] || \
  GITHUB_REPOSITORY="$(gh repo view --json nameWithOwner --jq '.nameWithOwner')"

RULESET_ID="$(gh api "repos/$GITHUB_REPOSITORY/rulesets" | jq -r \
  --arg name "$RULESET_NAME" '.[] | select(.name == $name) | .id' | head -1)"
[ -n "$RULESET_ID" ] || {
  echo "ERROR: ruleset '$RULESET_NAME' was not found in $GITHUB_REPOSITORY" >&2
  exit 1
}

CURRENT="$(gh api "repos/$GITHUB_REPOSITORY/rulesets/$RULESET_ID")"

if [ "$1" = "--apply" ]; then
  echo "[github_merge_ruleset] applying full-ruleset PUT; serialize repository-admin edits until this command completes." >&2
  ORIGINAL_UPDATED_AT="$(printf '%s\n' "$CURRENT" | jq -r '.updated_at')"
  PAYLOAD="$(printf '%s\n' "$CURRENT" | build_payload)"
  LATEST="$(gh api "repos/$GITHUB_REPOSITORY/rulesets/$RULESET_ID")"
  LATEST_UPDATED_AT="$(printf '%s\n' "$LATEST" | jq -r '.updated_at')"
  [ "$LATEST_UPDATED_AT" = "$ORIGINAL_UPDATED_AT" ] || {
    echo "ERROR: ruleset changed during update preparation; refusing to overwrite concurrent edits." >&2
    exit 1
  }
  printf '%s\n' "$PAYLOAD" | gh api --method PUT \
    "repos/$GITHUB_REPOSITORY/rulesets/$RULESET_ID" --input - >/dev/null
  CURRENT="$(gh api "repos/$GITHUB_REPOSITORY/rulesets/$RULESET_ID")"
fi

if ! printf '%s\n' "$CURRENT" | validate_ruleset_json; then
  echo "ERROR: '$RULESET_NAME' does not strictly require '$CERTIFICATION_CONTEXT'." >&2
  echo "Run: bash scripts/github_merge_ruleset.sh --apply" >&2
  exit 1
fi

echo "[github_merge_ruleset] OK: '$RULESET_NAME' strictly requires '$CERTIFICATION_CONTEXT'."
