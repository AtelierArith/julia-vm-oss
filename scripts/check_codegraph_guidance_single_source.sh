#!/usr/bin/env bash
# check_codegraph_guidance_single_source.sh — CodeGraph guidance must appear
# exactly once in per-session project context (Issue #9309).
#
# Background: Claude Code loads BOTH the root CLAUDE.md (a symlink to
# AGENTS.md, which carries the <!-- CODEGRAPH_START --> block) and
# .claude/CLAUDE.md. `codegraph init` (local Claude target) unconditionally
# (re-)writes the same block into .claude/CLAUDE.md, so every session carried
# the identical ~20-line guidance twice. The single source of truth is
# AGENTS.md; this audit fails when the marker block is duplicated into any
# other tracked agent-instruction file (most likely by a re-run of
# `codegraph init`).
#
# bash 3.2 compatible (macOS default shell).
set -euo pipefail
cd "$(dirname "$0")/.."

marker='CODEGRAPH_START'
fail=0

# 1. AGENTS.md is the designated single carrier of the CodeGraph block.
if ! grep -q "$marker" AGENTS.md; then
  echo "FAIL: AGENTS.md no longer contains the ${marker} block." >&2
  echo "  AGENTS.md is the single source for CodeGraph guidance (Issue #9309)." >&2
  echo "  Restore the <!-- CODEGRAPH_START --> ... <!-- CODEGRAPH_END --> section there." >&2
  fail=1
fi

# 2. Root CLAUDE.md must remain a symlink to AGENTS.md (not an independent
#    copy, which would duplicate the block again).
if [ ! -L CLAUDE.md ]; then
  echo "FAIL: CLAUDE.md is not a symlink to AGENTS.md." >&2
  echo "  A standalone CLAUDE.md re-duplicates the CodeGraph guidance (Issue #9309)." >&2
  fail=1
fi

# 3. No other agent-instruction file may carry the block as an independent
#    copy. Symlinks to AGENTS.md (e.g. GEMINI.md) are the sanctioned pattern
#    and are skipped — only regular-file copies duplicate context. A re-run
#    of `codegraph init` re-creates .claude/CLAUDE.md; the others guard
#    future installer targets.
for f in .claude/CLAUDE.md GEMINI.md .gemini/GEMINI.md .github/copilot-instructions.md; do
  if [ -L "$f" ]; then
    continue
  fi
  if [ -f "$f" ] && grep -q "$marker" "$f"; then
    echo "FAIL: $f duplicates the ${marker} block already carried by AGENTS.md." >&2
    echo "  Per-session context would include the CodeGraph guidance twice (Issue #9309)." >&2
    echo "  Remove the duplicated block (delete the file if that was its only content)." >&2
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  exit 1
fi

echo "OK: CodeGraph guidance has a single source (AGENTS.md)."
