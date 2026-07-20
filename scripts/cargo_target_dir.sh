#!/usr/bin/env bash
# Shared Cargo target-directory authority for shell harnesses (Issue #11695).
# This file is sourced; keep it compatible with macOS Bash 3.2.

resolve_cargo_target_dir() {
    local repo_root="$1"
    local metadata_json=""
    local metadata_target=""
    local fallback=""

    if command -v cargo >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1; then
        if metadata_json="$(
            cd "$repo_root" && cargo metadata --format-version 1 --no-deps 2>/dev/null
        )"; then
            metadata_target="$(
                printf '%s' "$metadata_json" |
                    python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])' 2>/dev/null
            )" || metadata_target=""
        fi
    fi

    if [ -n "$metadata_target" ]; then
        printf '%s\n' "$metadata_target"
        return 0
    fi

    fallback="${CARGO_TARGET_DIR:-$repo_root/target}"
    case "$fallback" in
        /*) ;;
        *) fallback="$repo_root/$fallback" ;;
    esac
    printf '%s\n' "$fallback"
}

