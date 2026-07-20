---
name: sjulia-document-workaround
description: >-
  Use when adding, updating, or removing a workaround, ad-hoc special case,
  temporary shortcut, or compatibility shim in SubsetJuliaVM (sjulia) — or
  when the user mentions WORKAROUNDS.md or a `Workaround:` comment.
---

# Document Ad-Hoc Workarounds in WORKAROUNDS.md

This skill enforces the **Workaround Management** rules from `AGENTS.md`
and `docs/vm/CODE_AUDITS.md`. An ad-hoc implementation is allowed
**only** when a tracking Issue exists and the workaround is fully registered in
`docs/vm/WORKAROUNDS.md`.

For filing the Issue itself (MWE, `bug` vs `unsupported-feature` labels), apply
`sjulia-report-gap` or the issue section in `sjulia-dev` **first**. This skill
starts **after** the Issue number is known.

## When a workaround is allowed

| Situation | Action |
|-----------|--------|
| Upstream `julia` runs it, sjulia cannot / runs wrong, and your task is **blocked** | Issue first → workaround + WORKAROUNDS.md |
| Structural upstream-compatible path is required (parser/lowering/macro/runtime) | **No** ad-hoc shortcut — implement upstream shape or file `unsupported-feature` |
| Package-specific hack when a general fix is needed | **Forbidden** unless the Issue explicitly scopes a temporary package shim |
| You dislike the implementation shape but sjulia is correct | Not a workaround — refactor normally, no WORKAROUNDS entry |

If you discover the gap while doing something else, still file the Issue before
the workaround — even when incidental.

## Mandatory checklist (adding a workaround)

Copy and complete before finishing the PR:

```
Workaround registration:
- [ ] GitHub Issue exists (#NNNN) with MWE + julia-vs-sjulia table
- [ ] In-code comment added with exact `(Issue #NNNN)` suffix
- [ ] Detailed section added to docs/vm/WORKAROUNDS.md
- [ ] Summary Table row added (next W-XX ID, category, file, impact, issue)
- [ ] bash scripts/check_workarounds_documented.sh — pass
- [ ] bash scripts/check_workarounds_sync.sh — pass
- [ ] PR body references Issue #NNNN
```

## Step 1 — In-code comment

Place the comment **on or immediately above** the workaround code.

| Language | Format |
|----------|--------|
| Rust | `// Workaround: <why / what is deferred> (Issue #NNNN)` |
| Julia | `# Workaround: <why / what is deferred> (Issue #NNNN)` |
| JavaScript (web) | `// Workaround: ... (Issue #NNNN)` |

Rules:

- `(Issue #NNNN)` is **required** — CI enforces it for Rust
  (`scripts/check_workarounds_documented.sh`, Issue #3179).
- Describe **why** the shortcut exists and **what** upstream behavior is
  preserved, not just "fix" or "hack".
- Do not use `Workaround:` in doc examples without an Issue link unless it is
  clearly quoted documentation (audit scripts exclude `///` and backtick
  patterns).

Example:

```rust
// Workaround: dynamic calls conservatively return Any when arg types are
// unknown at compile time (Issue #2425)
```

```julia
# Workaround: `::Module` does not win dispatch specificity over an untyped
# parameter, so branch on isa(x, Module) instead of a separate method (Issue #5005)
```

## Step 2 — WORKAROUNDS.md detailed section

Add a new `##` section **before** the Summary Table (active workarounds live
above "Resolved Workarounds"). Follow the existing document shape:

```markdown
## <Area> — <Short title>

**File:** `relative/path/to/file.ext` (and other files if needed)

```<lang>
// or # exact workaround comment + minimal surrounding context
```

**Impact:** Observable behavior today — what differs from the ideal upstream
shape, blast radius (types, dispatch, performance, API surface).

**Linked issue:** #NNNN (add related issues if any)

**Resolution path:** Concrete condition that lets you delete the workaround
(e.g. "once #5005 fixes Module dispatch specificity, split into two methods").
```

Section title pattern: `Compile — …`, `Base — …`, `VM — …`, `MacroTools — …`,
`Tests — …`, `Web Playground — …`, etc. Match nearby entries in
`docs/vm/WORKAROUNDS.md`.

Include a code snippet that shows the workaround comment (same text as in
source). Explain **Impact** in plain language for future readers removing the
shim.

## Step 3 — Summary Table row

Append a row to the **Summary Table** (still in active section, not Resolved):

```markdown
| W-XX | Category | `path/to/file` | One-line impact | #NNNN |
```

- **W-XX**: next unused ID in the table (currently scan for highest `W-` number).
- **Category**: short area (`Compile`, `Base`, `VM`, `MacroTools`, `Tests`, …).
- **File**: primary path, backtick-quoted, optional `:line`.
- **Impact**: one line — what users/maintainers should know.
- **Linked Issue**: `#NNNN` must appear in the doc (sync script greps `#${num}`).

Every Issue number in a Rust `// Workaround: ... (Issue #NNNN)` comment must
appear somewhere in `WORKAROUNDS.md` (`scripts/check_workarounds_sync.sh`,
Issue #3263). Julia `# Workaround:` comments are conventionally documented the
same way even though the sync script currently scans Rust only.

## Step 4 — Verify

From the repository root:

```bash
bash scripts/check_workarounds_documented.sh
bash scripts/check_workarounds_sync.sh
```

Fix any reported Issue numbers before opening the PR. Both scripts are registered
in CI (`docs/vm/CODE_AUDITS.md`).

Optional audit — find all active workaround comments:

```bash
rg -n --glob '*.rs' "// Workaround:" subset_julia_vm/src/
rg -n "# Workaround:" subset_julia_vm/src/ subset_julia_vm/packages/
```

## Removing a workaround (resolved)

When the root Issue is fixed:

1. **Delete** the in-code workaround comment and restore the upstream-shaped
   implementation.
2. **Remove** the detailed section and Summary Table row from active entries.
3. **Add** a row to **Resolved Workarounds** (`| PR | Issue | Description |`).
4. **Add** a regression fixture (or extend an existing one) proving the
   workaround is no longer needed.
5. Run both check scripts again.
6. Reference the fix PR and Issue in `docs/vm/DONE.md` / `STATUS.md` per
   post-PR policy.

Do not leave stale WORKAROUNDS entries — resolved shims belong only in the
Resolved table.

## Forbidden

- ❌ Workaround comment without `(Issue #NNNN)`.
- ❌ Issue number in code but missing from `WORKAROUNDS.md`.
- ❌ WORKAROUNDS.md entry with no matching in-code comment (orphan doc).
- ❌ Ad-hoc special case before the Issue exists (`sjulia-report-gap` applies).
- ❌ Skipping check scripts because the change "looks small".

## Quick reference

```
Issue filed (#NNNN)
  → comment in source (Issue #NNNN)
  → WORKAROUNDS.md section + Summary Table W-XX
  → both check scripts green
  → PR references #NNNN

Root cause fixed
  → remove comment + active doc
  → Resolved table + regression test
  → both check scripts green
```

Related skills: `sjulia-report-gap` (file Issue before workaround),
`sjulia-bug-prevention` (after fixing the underlying bug), `sjulia-logical-commits`
(group workaround + WORKAROUNDS.md + tests in one commit).
