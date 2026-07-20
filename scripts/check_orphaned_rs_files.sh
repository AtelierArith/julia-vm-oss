#!/usr/bin/env bash
# check_orphaned_rs_files.sh
#
# Detect Rust source files under a workspace crate's `src/` tree that no
# `mod`/`#[path]`/`include!` ever reaches — files that sit on disk,
# parse-valid, but are never fed to rustc, so edits to them have zero
# runtime effect while still showing up in grep/CodeGraph (Issue #10739:
# `subset_julia_vm/src/ir/core.rs` was exactly this for years after the
# Issue #8656 Core IR crate migration).
#
# See scripts/check_orphaned_rs_files.py for the resolution algorithm and
# its documented false-positive-avoidance bias (ambiguous cases are treated
# as reachable, not flagged).
#
# Usage (from repo root):
#   bash scripts/check_orphaned_rs_files.sh
#
# Exit code: 0 = no orphans (and no unresolved mod references), 1 = otherwise.
#
# Dependencies: python3 (stdlib only), bash 3.2+.

set -euo pipefail

cd "$(dirname "$0")/.."

python3 scripts/check_orphaned_rs_files.py
