#!/usr/bin/env bash
# aot_fixture_no_silent_mismatch.sh
#
# Property harness for Issue #7003: every VM-passing fixture should either
# compile and match under generated AoT output, or fail AoT compilation with a
# classified UnsupportedInstruction exit.
#
# With no fixture arguments, the script walks category manifests under
# subset_julia_vm/tests/fixtures/*/manifest.toml. Explicit fixture paths narrow
# the run to that subset.
#
# Requirements:
#   cargo build --release -p subset_julia_vm --features aot --bin juliars
#   cargo build --release -p subset_julia_vm --features repl --bin sjulia
# Binaries default under CARGO_TARGET_DIR; JULIARS_BIN/SJULIA_BIN override them.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/cargo_target_dir.sh"
cargo_target_dir="$(resolve_cargo_target_dir "$ROOT")"
export CARGO_TARGET_DIR="$cargo_target_dir"
JULIARS_BIN="${JULIARS_BIN:-$cargo_target_dir/release/juliars}"
SJULIA_BIN="${SJULIA_BIN:-$cargo_target_dir/release/sjulia}"
export JULIARS_BIN SJULIA_BIN

usage() {
    cat >&2 <<'EOF'
Usage:
  bash scripts/aot_fixture_no_silent_mismatch.sh [fixture.jl ...]

When no fixture paths are supplied, all category manifest fixture files are
checked. Unsupported AoT features are accepted only when juliars exits with the
UnsupportedInstruction exit code (5). Any generated-binary stdout mismatch,
final-value mismatch, generated binary failure, or unclassified compiler error
fails the run.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

if [[ ! -x "$JULIARS_BIN" ]]; then
    echo "ERROR: juliars binary not built. Run:" >&2
    echo "  cargo build --release -p subset_julia_vm --features aot --bin juliars" >&2
    exit 2
fi

if [[ ! -x "$SJULIA_BIN" ]]; then
    echo "ERROR: sjulia binary not built. Run:" >&2
    echo "  cargo build --release -p subset_julia_vm --features repl --bin sjulia" >&2
    exit 2
fi

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/sjulia-aot-fixture-property.XXXXXX")"
wrappers=()
cleanup() {
    rm -rf "$tmp_root"
    if [[ "${#wrappers[@]}" -gt 0 ]]; then
        rm -f "${wrappers[@]}"
    fi
}
trap cleanup EXIT

collect_manifest_fixtures() {
    find "$ROOT/subset_julia_vm/tests/fixtures" -mindepth 2 -maxdepth 2 \
        -name manifest.toml -print | sort | while IFS= read -r manifest; do
        manifest_dir="$(dirname "$manifest")"
        awk -v dir="$manifest_dir" '
            /^\[\[tests\]\]/ { file="" }
            /^[[:space:]]*file[[:space:]]*=/ {
                file=$0
                sub(/^[^"]*"/, "", file)
                sub(/".*$/, "", file)
                if (file != "") {
                    print dir "/" file
                }
            }
        ' "$manifest"
    done
}

resolve_fixture() {
    local candidate="$1"
    if [[ -f "$candidate" ]]; then
        printf '%s\n' "$candidate"
    elif [[ -f "$ROOT/$candidate" ]]; then
        printf '%s\n' "$ROOT/$candidate"
    else
        echo "ERROR: fixture not found: $candidate" >&2
        exit 2
    fi
}

write_result_wrapper() {
    local fixture="$1"
    local wrapper="$2"
    {
        printf 'println(begin\n'
        sed 's/^/    /' "$fixture"
        printf '\nend)\n'
    } >"$wrapper"
}

run_juliars_binary() {
    local fixture="$1"
    local generated_rs="$2"
    local aot_bin="$3"
    local log="$4"

    set +e
    timeout 1800 "$JULIARS_BIN" "$fixture" -o "$generated_rs" --emit-binary "$aot_bin" >"$log" 2>&1
    local status=$?
    set -e
    return "$status"
}

last_line() {
    local input="$1"
    local output="$2"
    awk 'END { print }' "$input" >"$output"
}

fixtures=()
if [[ "$#" -eq 0 ]]; then
    while IFS= read -r fixture; do
        fixtures+=("$fixture")
    done < <(collect_manifest_fixtures)
else
    for arg in "$@"; do
        fixtures+=("$(resolve_fixture "$arg")")
    done
fi

if [[ "${#fixtures[@]}" -eq 0 ]]; then
    echo "ERROR: no fixtures found" >&2
    exit 2
fi

checked=0
compiled=0
unsupported=0
skipped_vm=0

for fixture in "${fixtures[@]}"; do
    checked=$((checked + 1))
    case "$fixture" in
        "$ROOT"/*) display="${fixture#$ROOT/}" ;;
        *) display="$fixture" ;;
    esac

    work_dir="$tmp_root/case_$checked"
    mkdir -p "$work_dir"
    vm_stdout="$work_dir/vm.stdout"
    aot_stdout="$work_dir/aot.stdout"
    juliars_log="$work_dir/juliars.log"
    generated_rs="$work_dir/generated.rs"
    aot_bin="$work_dir/fixture_bin"

    if ! timeout 120 "$SJULIA_BIN" "$fixture" >"$vm_stdout" 2>&1; then
        skipped_vm=$((skipped_vm + 1))
        echo "SKIP: $display does not pass under release sjulia"
        continue
    fi

    if run_juliars_binary "$fixture" "$generated_rs" "$aot_bin" "$juliars_log"; then
        status=0
    else
        status=$?
    fi
    if [[ "$status" -ne 0 ]]; then
        if [[ "$status" -eq 5 ]]; then
            unsupported=$((unsupported + 1))
            echo "OK: $display is explicitly unsupported by AoT."
            continue
        fi
        echo "ERROR: juliars failed without UnsupportedInstruction for $display (exit $status)" >&2
        tail -40 "$juliars_log" >&2
        exit 1
    fi

    if ! timeout 120 "$aot_bin" >"$aot_stdout" 2>&1; then
        echo "ERROR: generated AoT binary failed for $display" >&2
        tail -40 "$aot_stdout" >&2
        exit 1
    fi

    if ! diff -u "$vm_stdout" "$aot_stdout"; then
        echo "MISMATCH: original fixture stdout differs for $display" >&2
        exit 1
    fi

    # Compare the fixture's final value separately. The original-source stdout
    # check above preserves explicit side-effect output; this wrapper check only
    # uses the final line so it is not masked by begin-wrapper side-effect gaps
    # tracked in Issue #7014.
    wrapper_dir="$(dirname "$fixture")"
    wrapper="$wrapper_dir/.aot_property_${checked}_$$.jl"
    wrappers+=("$wrapper")
    write_result_wrapper "$fixture" "$wrapper"

    wrapper_vm_stdout="$work_dir/wrapper.vm.stdout"
    wrapper_aot_stdout="$work_dir/wrapper.aot.stdout"
    wrapper_vm_last="$work_dir/wrapper.vm.last"
    wrapper_aot_last="$work_dir/wrapper.aot.last"
    wrapper_log="$work_dir/wrapper.juliars.log"
    wrapper_rs="$work_dir/wrapper.rs"
    wrapper_bin="$work_dir/wrapper_bin"

    if ! timeout 120 "$SJULIA_BIN" "$wrapper" >"$wrapper_vm_stdout" 2>&1; then
        echo "ERROR: final-value wrapper failed under release sjulia for $display" >&2
        tail -40 "$wrapper_vm_stdout" >&2
        exit 1
    fi

    if run_juliars_binary "$wrapper" "$wrapper_rs" "$wrapper_bin" "$wrapper_log"; then
        status=0
    else
        status=$?
    fi
    if [[ "$status" -ne 0 ]]; then
        echo "ERROR: final-value wrapper did not compile after original fixture compiled for $display (exit $status)" >&2
        tail -40 "$wrapper_log" >&2
        exit 1
    fi

    if ! timeout 120 "$wrapper_bin" >"$wrapper_aot_stdout" 2>&1; then
        echo "ERROR: generated AoT final-value wrapper binary failed for $display" >&2
        tail -40 "$wrapper_aot_stdout" >&2
        exit 1
    fi

    last_line "$wrapper_vm_stdout" "$wrapper_vm_last"
    last_line "$wrapper_aot_stdout" "$wrapper_aot_last"
    if ! diff -u "$wrapper_vm_last" "$wrapper_aot_last"; then
        echo "MISMATCH: final fixture value differs for $display" >&2
        exit 1
    fi

    compiled=$((compiled + 1))
    echo "OK: $display AoT stdout and final value match VM."
done

echo "OK: checked $checked fixture(s): $compiled compiled+matched, $unsupported explicitly unsupported, $skipped_vm skipped because VM failed."
