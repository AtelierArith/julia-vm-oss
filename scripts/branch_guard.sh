#!/usr/bin/env bash
# branch_guard.sh
#
# Fail when the current git branch is `main` or `master`. Used as a
# pre-commit guard (or invoked by hand) in the /auto-work workflow to
# prevent the recurring "edited and committed on main by accident"
# pattern (Issue #4798).
#
# NAMING: deliberately NOT named `check_*.sh` so it does NOT trip the
# `Verify all check_*.sh scripts are referenced in this workflow and
# docs` audit (same convention as `scripts/fixture_julia_parity.sh`
# and `scripts/probe_base_api_parity.sh`). This is a developer-side
# guard, not a CI gate.
#
# Usage:
#   bash scripts/branch_guard.sh                  # exit 1 if on main/master
#   bash scripts/branch_guard.sh --quiet          # same, but no message on success
#
# Recommended pre-commit hook installation (run once per clone):
#   echo '#!/usr/bin/env bash' > .git/hooks/pre-commit
#   echo 'exec bash scripts/branch_guard.sh --quiet' >> .git/hooks/pre-commit
#   chmod +x .git/hooks/pre-commit
#
# Recovery (if you already committed on main locally but haven't pushed):
#   git branch fix/<NNNN>-<slug>
#   git reset --hard origin/main
#   git checkout fix/<NNNN>-<slug>

set -euo pipefail

QUIET=0
for arg in "$@"; do
    case "$arg" in
        --quiet|-q) QUIET=1 ;;
        *)
            echo "unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

current="$(git symbolic-ref --short HEAD 2>/dev/null || echo "DETACHED")"

case "$current" in
    main|master)
        echo "ERROR: refusing to operate on '$current' — create a feature branch first" >&2
        echo "  git checkout -b fix/<issue-number>-<short-slug>" >&2
        echo "" >&2
        echo "If you already committed on main locally (and haven't pushed), recover with:" >&2
        echo "  git branch fix/<NNNN>-<slug>" >&2
        echo "  git reset --hard origin/main" >&2
        echo "  git checkout fix/<NNNN>-<slug>" >&2
        exit 1
        ;;
    DETACHED)
        echo "ERROR: HEAD is detached — not a regular branch" >&2
        exit 1
        ;;
    *)
        if [[ "$QUIET" == "0" ]]; then
            echo "OK: on feature branch '$current'"
        fi
        exit 0
        ;;
esac
