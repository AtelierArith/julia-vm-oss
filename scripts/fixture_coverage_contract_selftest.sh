#!/usr/bin/env bash
# Isolated contract tests for check_unregistered_fixtures.sh (Issue #11041).

set -eu

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECKER="$SCRIPT_DIR/check_unregistered_fixtures.sh"
SANDBOX="$(mktemp -d "${TMPDIR:-/tmp}/sjulia-fixture-coverage.XXXXXX")"
trap 'rm -rf "$SANDBOX"' EXIT HUP INT TERM

FIXTURES="$SANDBOX/fixtures"
ALLOWLIST="$SANDBOX/allowlist.tsv"

reset_case() {
    rm -rf "$FIXTURES"
    mkdir -p "$FIXTURES/sandbox"
    : > "$ALLOWLIST"
    printf '%s\n' '[[tests]]' 'name = "sandbox_main_11041"' 'file = "main.jl"' 'expected = "true"' \
        > "$FIXTURES/sandbox/manifest.toml"
    printf '%s\n' 'true' > "$FIXTURES/sandbox/helper.jl"
}

run_checker() {
    SJULIA_FIXTURE_COVERAGE_FIXTURES_DIR="$FIXTURES" \
        SJULIA_FIXTURE_COVERAGE_ALLOWLIST="$ALLOWLIST" \
        bash "$CHECKER"
}

expect_pass() {
    label="$1"
    if ! output="$(run_checker 2>&1)"; then
        printf 'FAIL: %s unexpectedly failed:\n%s\n' "$label" "$output" >&2
        exit 1
    fi
    printf 'PASS: %s\n' "$label"
}

expect_fail() {
    label="$1"
    reason="$2"
    if output="$(run_checker 2>&1)"; then
        printf 'FAIL: %s unexpectedly passed:\n%s\n' "$label" "$output" >&2
        exit 1
    fi
    if ! printf '%s\n' "$output" | grep -Fq -- "$reason"; then
        printf 'FAIL: %s failed without expected reason %s:\n%s\n' "$label" "$reason" "$output" >&2
        exit 1
    fi
    printf 'PASS: %s\n' "$label"
}

# A computed path is intentionally invisible to the literal include scanner.
reset_case
printf '%s\n' 'include(joinpath(@__DIR__, "helper.jl"))' 'true' > "$FIXTURES/sandbox/main.jl"
expect_fail "unregistered computed-path helper" "sandbox/helper.jl"

# The same computed helper is covered only by an explicit, justified row.
printf '%s\t%s\n' 'sandbox/helper.jl' 'dynamic-include-helper: computed by joinpath in the sandbox fixture' > "$ALLOWLIST"
expect_pass "justified computed-path helper"

# Literal include targets are discovered automatically and must not be allowlisted.
reset_case
printf '%s\n' 'include("helper.jl")' 'true' > "$FIXTURES/sandbox/main.jl"
expect_pass "literal include helper auto-detection"

# evalfile is a separate scanner branch and carries the same auto-detection contract.
reset_case
printf '%s\n' 'evalfile("helper.jl")' 'true' > "$FIXTURES/sandbox/main.jl"
expect_pass "literal evalfile helper auto-detection"

# Text in comments, block comments, strings, and longer identifiers is not a call.
reset_case
printf '%s\n' \
    '# include("helper.jl")' \
    '#= evalfile("helper.jl") =#' \
    'notice = "include(\"helper.jl\")"' \
    'fakeinclude("helper.jl")' \
    'include(joinpath(@__DIR__, "helper.jl"))' \
    'true' > "$FIXTURES/sandbox/main.jl"
expect_fail "non-executable literal-looking text" "sandbox/helper.jl"

reset_case
printf '%s\n' \
    'x′include(path) = true' \
    'x′include("helper.jl")' \
    'include(joinpath(@__DIR__, "helper.jl"))' \
    'true' > "$FIXTURES/sandbox/main.jl"
expect_fail "Unicode-prefixed identifier is not include" "sandbox/helper.jl"

reset_case
printf '%s\n' \
    'unused = :(include("helper.jl"))' \
    'unused_block = quote' \
    '    ∂end = 1' \
    '    if true' \
    '        include("helper.jl")' \
    '    end' \
    'end' \
    'include(joinpath(@__DIR__, "helper.jl"))' \
    'true' > "$FIXTURES/sandbox/main.jl"
expect_fail "quoted include expressions are not executable" "sandbox/helper.jl"

reset_case
printf '%s\n' 'marker = '\''#'\''; include("helper.jl")' 'true' > "$FIXTURES/sandbox/main.jl"
expect_pass "character literal before same-line include"

reset_case
# shellcheck disable=SC2016 # Literal Julia interpolation source.
printf '%s\n' 'message = "$(include("helper.jl"))"' 'true' > "$FIXTURES/sandbox/main.jl"
expect_pass "literal include in string interpolation"

reset_case
# shellcheck disable=SC2016 # Literal Julia string-macro source.
printf '%s\n' \
    'raw_text = raw"""$(include("helper.jl"))"""' \
    'regex_text = r"""$(include("helper.jl"))"""' \
    'include(joinpath(@__DIR__, "helper.jl"))' \
    'true' > "$FIXTURES/sandbox/main.jl"
expect_fail "nonstandard string literals do not interpolate" "sandbox/helper.jl"

# Allowlist rows remain two-sided and require a non-empty explanation.
reset_case
printf '%s\n' 'include(joinpath(@__DIR__, "helper.jl"))' 'true' > "$FIXTURES/sandbox/main.jl"
printf 'sandbox/helper.jl\t\n' > "$ALLOWLIST"
expect_fail "missing allowlist reason" "allowlist row is missing a reason"

printf '%s\t\n' 'sandbox/helper.jl' > "$ALLOWLIST"
printf '%s\t%s\n' 'sandbox/helper.jl' 'later reason must not overwrite the empty row' >> "$ALLOWLIST"
expect_fail "duplicate path cannot hide a missing reason" "duplicate allowlist path"

reset_case
printf '%s\n' 'include("helper.jl")' 'true' > "$FIXTURES/sandbox/main.jl"
printf '%s\t%s\n' 'sandbox/helper.jl' 'stale literal-helper exemption' > "$ALLOWLIST"
expect_fail "stale covered allowlist row" "now registered/referenced; remove this row"

reset_case
printf '%s\n' 'evalfile("helper.jl")' 'true' > "$FIXTURES/sandbox/main.jl"
printf '%s\t%s\n' 'sandbox/helper.jl' 'stale literal-evalfile exemption' > "$ALLOWLIST"
expect_fail "stale evalfile allowlist row" "now registered/referenced; remove this row"

reset_case
printf '%s\n' 'true' > "$FIXTURES/sandbox/main.jl"
printf '%s\t%s\n' 'sandbox/missing.jl' 'stale missing-file exemption' > "$ALLOWLIST"
expect_fail "stale missing-file allowlist row" "file does not exist"

echo "OK: fixture coverage literal/computed-path contracts are self-tested (Issue #11041)."
