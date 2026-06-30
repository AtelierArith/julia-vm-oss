# Workarounds Historically Tagged as Issue 1891

This document centralizes workaround notes that were previously repeated inline as
`(Issue #1891)` comments across the codebase.

## Scope

These workarounds are transitional behavior in areas such as:
- lowering edge cases
- compile-time type/dispatch approximations
- VM/runtime fallbacks
- stdlib compatibility shims

## Maintenance Rule

When touching workaround code that used to carry the inline `Issue #1891` suffix:
1. Keep the local comment focused on behavior and constraints.
2. Track cross-cutting status in this document (or a dedicated follow-up issue).
3. Avoid duplicating the same issue-tag suffix in many files.

## Current Tracking

To list files that still mention the historical tag directly:

```bash
rg -n "Issue #1891|#1891" subset_julia_vm/src subset_julia_vm/tests
```

At the time of consolidation for issue #2875, repeated inline tag suffixes were
removed and tracking was centralized here.
