#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ "$#" -eq 0 ]; then
    set -- $(git -C "$ROOT" diff --name-only -- \
        'subset_julia_vm/tests/fixtures/*/*.jl' \
        'subset_julia_vm/tests/fixtures/*/manifest.toml')
fi

if [ "$#" -eq 0 ]; then
    echo "No fixture paths supplied or changed."
    echo "Usage: scripts/fixture_fast_feedback.sh subset_julia_vm/tests/fixtures/<category>/<file>.jl ..."
    exit 0
fi

tmp="$(mktemp "${TMPDIR:-/tmp}/fixture-fast-feedback.XXXXXX")"
trap 'rm -f "$tmp"' EXIT

for path in "$@"; do
    case "$path" in
        subset_julia_vm/tests/fixtures/*/*.jl)
            category="${path#subset_julia_vm/tests/fixtures/}"
            category="${category%%/*}"
            file="${path##*/}"
            manifest="subset_julia_vm/tests/fixtures/${category}/manifest.toml"
            test_name="$(
                awk -v want="$file" '
                    /^\[\[tests\]\]/ { name=""; file="" }
                    /^[[:space:]]*name[[:space:]]*=/ {
                        name=$0
                        sub(/^[^"]*"/, "", name)
                        sub(/".*$/, "", name)
                    }
                    /^[[:space:]]*file[[:space:]]*=/ {
                        file=$0
                        sub(/^[^"]*"/, "", file)
                        sub(/".*$/, "", file)
                    }
                    file == want && name != "" {
                        print name
                        exit
                    }
                ' "$ROOT/$manifest"
            )"
            if [ -z "$test_name" ]; then
                test_name="(manifest entry not found)"
            fi
            printf '%s\t%s\t%s\t%s\n' "$category" "$path" "$test_name" "$manifest" >> "$tmp"
            ;;
    esac
done

if [ ! -s "$tmp" ]; then
    echo "No Julia fixture files found in input."
    echo "Usage: scripts/fixture_fast_feedback.sh subset_julia_vm/tests/fixtures/<category>/<file>.jl ..."
    exit 0
fi

echo "# Fast fixture feedback commands"
echo
echo "# 1. Upstream Julia parity for changed fixtures"
while IFS="$(printf '\t')" read -r _category path _test_name _manifest; do
    echo "julia --startup-file=no --history-file=no $path"
done < "$tmp"

echo
echo "# 2. Refresh release sjulia once before direct fixture checks"
echo "timeout 1800 cargo build --release --bin sjulia --features repl"

echo
echo "# 3. Direct sjulia fixture checks"
while IFS="$(printf '\t')" read -r _category path _test_name _manifest; do
    echo "timeout 180 target/release/sjulia $path"
done < "$tmp"

echo
echo "# 4. Relevant category nextest gates"
cut -f1 "$tmp" | sort -u | while read -r category; do
    echo "timeout 1800 cargo nextest run --release --test fixture_tests ${category}:: --no-fail-fast"
done

echo
echo "# 4b. Smoke tier (Issue #9671 Phase 4): changed categories + representative"
echo "#     cross-cutting categories that historically surface dispatch/inference/"
echo "#     promotion interactions (the #5966 one-process-interaction class). This is"
echo "#     an inner-loop check ONLY — the FULL suite remains the merge gate."
# Representative cross-cutting categories + whatever changed above, de-duplicated.
smoke_categories="$(
    {
        printf '%s\n' dispatch type_inference types promotion iteration numeric strings
        cut -f1 "$tmp"
    } | sort -u | tr '\n' ' '
)"
smoke_filter="$(
    for c in $smoke_categories; do printf '%s:: ' "$c"; done
)"
echo "timeout 1800 cargo nextest run --release --test fixture_tests ${smoke_filter}--no-fail-fast"

echo
echo "# 5. iOS gates when VM/compiler/runtime behavior changed"
echo "timeout 1800 cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios-sim"
echo "timeout 1800 xcodebuild -project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj -scheme SubsetJuliaVMApp -sdk iphonesimulator -destination 'platform=iOS Simulator,name=iPad (A16)' build"

echo
echo "# Manifest mapping"
while IFS="$(printf '\t')" read -r category path test_name manifest; do
    echo "${category}: ${test_name} (${path}, ${manifest})"
done < "$tmp"

echo
echo "# Run the commands sequentially; do not run cargo build and nextest concurrently."
