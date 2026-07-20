#!/usr/bin/env bash
# check_vendored_drift.sh — vendor/ fork upstream drift check (Issue #9000)
#
# Purpose:
#   SubsetJuliaVM vendors a locally-patched fork of astro-float-num in
#   vendor/astro-float-num/ (Issue #6794). Since vendored forks don't receive
#   upstream security/bug fixes automatically, this script checks whether the
#   upstream crate has released a NEW version beyond what we've pinned and
#   prints a drift report.
#
#   Run quarterly (the nightly supply-chain job runs this weekly as a reminder),
#   or after any upstream release of astro-float-num.
#
#   A non-zero exit means a NEW upstream release was found — this is a WARNING,
#   not necessarily a bug. A human must decide whether to:
#     (a) re-patch the new upstream version and update vendor/astro-float-num/
#     (b) stay on the current patched version (acceptable if no security/bugs)
#     (c) remove the fork entirely and accept the upstream fix (if Issue #6794
#         was resolved upstream)
#
# Exit codes:
#   0 — no drift (upstream version matches our pinned version, or drift check
#       could not run because curl/python3 are unavailable — non-fatal)
#   1 — upstream has a newer release than our pinned version (human review needed)
#
# Registration (Issue #3112):
#   Registered in docs/vm/CODE_AUDITS.md § Audit Policies.
#   Runs in nightly-gates.yml `supply-chain` job.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

VENDOR_CARGO="$REPO_ROOT/vendor/astro-float-num/Cargo.toml"
if [[ ! -f "$VENDOR_CARGO" ]]; then
  echo "ERROR: vendor/astro-float-num/Cargo.toml not found" >&2
  exit 1
fi

# Extract the pinned version from our vendored Cargo.toml
PINNED_VERSION="$(grep '^version' "$VENDOR_CARGO" | head -1 | sed 's/.*= *"\(.*\)".*/\1/')"
if [[ -z "$PINNED_VERSION" ]]; then
  echo "ERROR: could not parse version from $VENDOR_CARGO" >&2
  exit 1
fi
echo "Vendored version: astro-float-num v$PINNED_VERSION"

# Query crates.io for the latest published version of astro-float-num
# (which is the upstream source for the fork).
if ! command -v curl &>/dev/null; then
  echo "SKIP: curl not available; cannot query crates.io. Drift check skipped."
  exit 0
fi

CRATES_IO_URL="https://crates.io/api/v1/crates/astro-float-num"
RESPONSE="$(curl --silent --fail --max-time 15 \
  --header "User-Agent: SubsetJuliaVM/check_vendored_drift (Issue #9000)" \
  "$CRATES_IO_URL" 2>&1)" || {
  echo "SKIP: crates.io query failed (network unavailable?). Drift check skipped."
  exit 0
}

# Extract latest version using python3 (always available in CI)
if ! command -v python3 &>/dev/null; then
  echo "SKIP: python3 not available; cannot parse crates.io JSON. Drift check skipped."
  exit 0
fi

LATEST_VERSION="$(echo "$RESPONSE" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    print(data['crate']['newest_version'])
except Exception as e:
    print('ERROR: ' + str(e), file=sys.stderr)
    sys.exit(1)
")"

if [[ -z "$LATEST_VERSION" ]]; then
  echo "SKIP: could not parse latest version from crates.io response."
  exit 0
fi

echo "Upstream latest: astro-float-num v$LATEST_VERSION"

if [[ "$PINNED_VERSION" == "$LATEST_VERSION" ]]; then
  echo "OK: vendored astro-float-num v$PINNED_VERSION is current with upstream."
  exit 0
else
  echo ""
  echo "DRIFT DETECTED: vendor/astro-float-num is at v$PINNED_VERSION"
  echo "                upstream crates.io has v$LATEST_VERSION"
  echo ""
  echo "Action required (Issue #9000 policy):"
  echo "  1. Review upstream changelog: https://github.com/stencillogic/astro-float/releases"
  echo "  2. Determine if security or bug fixes are present."
  echo "  3. If yes: re-apply the Issue #6794 patch to the new version."
  echo "     Update vendor/astro-float-num/ and bump the [patch.crates-io] version."
  echo "  4. If no: document the decision in docs/vm/SUPPLY_CHAIN.md and"
  echo "     update the 'last_reviewed' date in the Vendored Fork Tracker table."
  echo "  5. Update the 'next_review' date."
  echo ""
  echo "See docs/vm/SUPPLY_CHAIN.md for the full vendored fork tracking policy."
  exit 1
fi
