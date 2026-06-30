#!/usr/bin/env bash
#
# End-to-end driver for the SubsetJuliaVM iOS REPL.
#
# Boots a Simulator, (optionally) builds & installs the app, launches it, then
# pastes a block of Julia code into the REPL, runs it, and captures a screenshot
# — the automated version of the manual flow used to reproduce/verify REPL bugs
# (e.g. Issue #8214). The actual UI automation lives in `ios_repl_paste.py`
# (Quartz CGEvents + accessibility-tree element lookup); this wrapper handles the
# simctl/xcodebuild plumbing.
#
# Requirements:
#   * uv            (runs the Python driver; installs pyobjc on demand)
#   * Xcode + a simulator runtime
#   * macOS Accessibility permission for the controlling terminal/app
#     (System Settings → Privacy & Security → Accessibility)
#
# NOTE: --build only rebuilds the Swift app; it reuses the committed
# SubsetJuliaVM.xcframework. To pick up Rust/VM (or bundled-package .jl) changes,
# rebuild the framework first with ./build.sh.
#
# Usage:
#   scripts/ios_repl_e2e.sh --code-file snippet.jl --screenshot out.png
#   scripts/ios_repl_e2e.sh --build --code 'using Plots; plot(sin)' --screenshot out.png
#   scripts/ios_repl_e2e.sh --dump-ax        # just print the REPL accessibility tree
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEVICE_NAME="iPad (A16)"
BUNDLE_ID="jp.atelier-arith.subsetjuliavm"
DERIVED_DATA="${REPO_ROOT}/SubsetJuliaVMApp/.e2e-derived-data"
DO_BUILD=0
PASS_ARGS=()

usage() { sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --build) DO_BUILD=1; shift ;;
    --device) DEVICE_NAME="$2"; shift 2 ;;
    --bundle-id) BUNDLE_ID="$2"; PASS_ARGS+=(--bundle-id "$2"); shift 2 ;;
    --derived-data) DERIVED_DATA="$2"; shift 2 ;;
    --code-file) PASS_ARGS+=(--code-file "$2"); shift 2 ;;
    --code) PASS_ARGS+=(--code "$2"); shift 2 ;;
    --screenshot) PASS_ARGS+=(--screenshot "$2"); shift 2 ;;
    --wait) PASS_ARGS+=(--wait "$2"); shift 2 ;;
    --no-run) PASS_ARGS+=(--no-run); shift ;;
    --launch) PASS_ARGS+=(--launch); shift ;;
    --dump-ax) PASS_ARGS+=(--dump-ax); shift ;;
    -h|--help) usage 0 ;;
    *) echo "unknown arg: $1" >&2; usage 1 ;;
  esac
done

command -v uv >/dev/null 2>&1 || { echo "error: 'uv' is required (https://docs.astral.sh/uv/)" >&2; exit 1; }

echo "==> resolving simulator: ${DEVICE_NAME}"
# awk match() with a capture-group array is a GNU extension; macOS ships BSD awk,
# where it is a syntax error. Under `set -e`/`pipefail` that aborts the whole
# script before the fallback below, so tolerate the failure (`|| true`) and keep
# the portable grep/sed path as the real resolver.
DEVICE_UDID="$(xcrun simctl list devices available | awk -v name="${DEVICE_NAME} (" 'index($0, name){ if (match($0, /\(([-0-9A-Fa-f]{36})\)/, m)) { print m[1]; exit } }' 2>/dev/null || true)"
if [[ -z "${DEVICE_UDID}" ]]; then
  # Portable fallback (BSD awk / no GNU match): grep the line, sed out the UDID.
  DEVICE_UDID="$(xcrun simctl list devices available | grep -F "${DEVICE_NAME} (" | head -1 | sed -E 's/.*\(([-0-9A-Fa-f]{36})\).*/\1/')"
fi
[[ -n "${DEVICE_UDID}" ]] || { echo "error: no available simulator named '${DEVICE_NAME}'" >&2; exit 1; }
echo "    udid=${DEVICE_UDID}"

echo "==> booting simulator (if needed)"
xcrun simctl bootstatus "${DEVICE_UDID}" -b >/dev/null 2>&1 || xcrun simctl boot "${DEVICE_UDID}" || true
xcrun simctl bootstatus "${DEVICE_UDID}" -b >/dev/null 2>&1 || true
open -a Simulator || true

if [[ "${DO_BUILD}" == "1" ]]; then
  echo "==> building app (Debug, iphonesimulator)"
  xcodebuild \
    -project "${REPO_ROOT}/SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj" \
    -scheme SubsetJuliaVMApp -sdk iphonesimulator -configuration Debug \
    -destination "platform=iOS Simulator,name=${DEVICE_NAME}" \
    -derivedDataPath "${DERIVED_DATA}" build
  APP_PATH="${DERIVED_DATA}/Build/Products/Debug-iphonesimulator/SubsetJuliaVMApp.app"
  echo "==> installing ${APP_PATH}"
  xcrun simctl install "${DEVICE_UDID}" "${APP_PATH}"
  PASS_ARGS+=(--launch)
fi

echo "==> driving REPL"
exec uv run --quiet "${REPO_ROOT}/scripts/ios_repl_paste.py" --device "${DEVICE_UDID}" "${PASS_ARGS[@]}"
