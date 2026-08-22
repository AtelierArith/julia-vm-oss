#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 ]]; then
  printf 'usage: %s <seconds> <log-path> <command> [args...]\n' "$0" >&2
  exit 2
fi

seconds=$1
log_path=$2
shift 2

mkdir -p "$(dirname "$log_path")"
set +e
/opt/homebrew/bin/gtimeout --signal=TERM --kill-after=10s "$seconds" "$@" >"$log_path" 2>&1
status=$?
set -e

if [[ $status -eq 124 ]]; then
  printf 'TIMEOUT after %ss: %s\nlog: %s\n' "$seconds" "$*" "$log_path" >&2
elif [[ $status -ne 0 ]]; then
  printf 'FAILED (%s): %s\nlog: %s\n' "$status" "$*" "$log_path" >&2
fi

exit "$status"
