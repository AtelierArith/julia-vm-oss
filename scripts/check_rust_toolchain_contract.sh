#!/usr/bin/env bash
# Enforce the pinned reference toolchain and feature-gated Clippy lanes (Issue #11253).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "FAIL: Rust toolchain contract: $*" >&2
  exit 1
}

require_exact_line() {
  local file="$1" line="$2" reason="$3"
  grep -Fqx -- "$line" "$file" || fail "$reason"
}

[ -f rust-toolchain.toml ] || fail "rust-toolchain.toml is missing"
require_exact_line rust-toolchain.toml 'channel = "1.95.0"' \
  "reference toolchain must remain Rust 1.95.0"
require_exact_line rust-toolchain.toml 'profile = "minimal"' \
  "rust-toolchain.toml must use the minimal profile"
require_exact_line rust-toolchain.toml 'components = ["clippy", "rustfmt"]' \
  "rust-toolchain.toml must install clippy and rustfmt"

require_exact_line Cargo.toml 'rust-version = "1.95"' \
  "workspace MSRV must remain explicit at Rust 1.95"

WORKSPACE_MANIFESTS=()
while IFS= read -r manifest; do
  WORKSPACE_MANIFESTS+=("$manifest")
done < <(
  awk '
    /^members = \[/ { in_members = 1; next }
    in_members && /^\]/ { exit }
    in_members {
      line = $0
      sub(/#.*/, "", line)
      gsub(/[[:space:]\",]/, "", line)
      if (line != "") print line "/Cargo.toml"
    }
  ' Cargo.toml
)
[ "${#WORKSPACE_MANIFESTS[@]}" -gt 0 ] || fail "could not enumerate workspace members"
for manifest in "${WORKSPACE_MANIFESTS[@]}"; do
  [ -f "$manifest" ] || fail "workspace manifest disappeared: $manifest"
  require_exact_line "$manifest" 'rust-version.workspace = true' \
    "$manifest must inherit workspace.package.rust-version"
done

[ -x scripts/run_clippy_lanes.sh ] || fail "scripts/run_clippy_lanes.sh must be executable"
expected="$(mktemp)"
actual="$(mktemp)"
lint_job="$(mktemp)"
trap 'rm -f "$expected" "$actual" "$lint_job"' EXIT
cat > "$expected" <<'EOF'
lane	scope	features	command
default	workspace	none	cargo clippy --locked --workspace --all-targets -- -D warnings
repl	subset_julia_vm	repl	cargo clippy --locked -p subset_julia_vm --features repl --all-targets -- -D warnings
aot	subset_julia_vm	aot	cargo clippy --locked -p subset_julia_vm --features aot --all-targets -- -D warnings
aot-cranelift	subset_julia_vm	aot,cranelift	cargo clippy --locked -p subset_julia_vm --features aot,cranelift --all-targets -- -D warnings
EOF
bash scripts/run_clippy_lanes.sh --list > "$actual"
if ! diff -u "$expected" "$actual"; then
  fail "mandatory default/repl/aot/aot-cranelift lane enumeration changed"
fi

grep -Fq 'bash scripts/run_clippy_lanes.sh default' scripts/premerge_gate.sh ||
  fail "premerge_gate.sh must invoke the registered 'default' Clippy lane"
# shellcheck disable=SC2016  # Match the literal ROOT reference in the owner script.
grep -Fq 'bash "$ROOT/scripts/run_clippy_lanes.sh" aot' scripts/test_aot.sh ||
  fail "test_aot.sh must invoke the registered 'aot' Clippy lane"
# shellcheck disable=SC2016  # Match the literal temporary-project path.
grep -Fq 'cargo clippy --manifest-path "$tmp_dir/Cargo.toml" -- -D warnings' scripts/test_aot.sh ||
  fail "test_aot.sh must retain the generated-Rust Clippy smoke lane"

awk '
  /^  lint:/ { in_job = 1; print; next }
  in_job && /^  [[:alnum:]_-]+:/ { exit }
  in_job { print }
' .github/workflows/ci.yml > "$lint_job"
grep -Fq 'uses: dtolnay/rust-toolchain@stable' "$lint_job" ||
  fail "CI must retain a current-stable moving toolchain lane"
grep -Fq 'components: clippy' "$lint_job" ||
  fail "CI current-stable lint job must install Clippy"
grep -Fq 'RUSTUP_TOOLCHAIN: stable' "$lint_job" ||
  fail "CI must override the checked-in reference pin for its current-stable lane"
grep -Fq 'run: bash scripts/run_clippy_lanes.sh' "$lint_job" ||
  fail "CI current-stable lint job must execute every registered Clippy lane"

require_exact_line docs/vm/RUST_TOOLCHAIN.md 'rustc -Vv' \
  "RUST_TOOLCHAIN.md must document exact rustc reproduction output"
require_exact_line docs/vm/RUST_TOOLCHAIN.md 'cargo clippy -V' \
  "RUST_TOOLCHAIN.md must document exact Clippy reproduction output"

echo "OK: Rust 1.95 MSRV/reference pin and all mandatory Clippy lanes are intact (Issue #11253)."
