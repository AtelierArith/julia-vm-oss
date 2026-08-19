#!/usr/bin/env bash
# Canonical executable registry for workspace-owned Clippy lanes (Issue #11253).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

LANES=(default repl aot aot-wasm aot-cranelift)
CARGO_ARGS=()
SCOPE=""
FEATURES=""

configure_lane() {
  case "$1" in
    default)
      SCOPE="workspace"
      FEATURES="none"
      CARGO_ARGS=(clippy --locked --workspace --all-targets -- -D warnings)
      ;;
    repl)
      SCOPE="subset_julia_vm"
      FEATURES="repl"
      CARGO_ARGS=(clippy --locked -p subset_julia_vm --features repl --all-targets -- -D warnings)
      ;;
    aot)
      SCOPE="subset_julia_vm"
      FEATURES="aot"
      CARGO_ARGS=(clippy --locked -p subset_julia_vm --features aot --all-targets -- -D warnings)
      ;;
    aot-wasm)
      SCOPE="subset_julia_vm"
      FEATURES="aot,aot-wasm"
      CARGO_ARGS=(clippy --locked -p subset_julia_vm --features "aot,aot-wasm" --all-targets -- -D warnings)
      ;;
    aot-cranelift)
      SCOPE="subset_julia_vm"
      FEATURES="aot,cranelift"
      CARGO_ARGS=(clippy --locked -p subset_julia_vm --features "aot,cranelift" --all-targets -- -D warnings)
      ;;
    *)
      echo "FAIL: unknown Clippy lane '$1'" >&2
      return 2
      ;;
  esac
}

print_lane() {
  local lane="$1" arg command="cargo"
  configure_lane "$lane"
  for arg in "${CARGO_ARGS[@]}"; do
    command="$command $arg"
  done
  printf '%s\t%s\t%s\t%s\n' "$lane" "$SCOPE" "$FEATURES" "$command"
}

if [ "${1:-}" = "--list" ]; then
  [ "$#" -eq 1 ] || {
    echo "FAIL: --list does not accept lane arguments" >&2
    exit 2
  }
  printf 'lane\tscope\tfeatures\tcommand\n'
  for lane in "${LANES[@]}"; do
    print_lane "$lane"
  done
  exit 0
fi

if [ "$#" -eq 0 ]; then
  set -- "${LANES[@]}"
fi

for lane in "$@"; do
  configure_lane "$lane"
  echo "== Clippy lane: $lane (scope=$SCOPE, features=$FEATURES) =="
  timeout 1800 cargo "${CARGO_ARGS[@]}"
done
