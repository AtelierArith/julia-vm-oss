---
name: github-milestone-triage
description: Use when triaging open AtelierArith/ailujsoi GitHub issues that lack a milestone assignment.
---

# GitHub Milestone Triage for ailujsoi

## Overview

Assign milestone-unassigned open issues to the best-fit existing milestone, or propose a new milestone when an issue clearly belongs to a missing category. Prefer the most specific milestone and document the reason.

## When to Use

- A user asks you to triage open issues without milestones.
- You are grooming the backlog and milestones exist but are under-used.
- You need to decide whether a new milestone category is justified.

## Core Pattern

1. **List unassigned open issues**
   ```bash
   gh issue list --state open --milestone none --limit 200
   ```
2. **List existing milestones with context**
   ```bash
   gh api repos/AtelierArith/ailujsoi/milestones --paginate \
     --jq '.[] | "\(.title): \(.description // "no description") (open:\(.open_issues))"'
   ```
3. **Classify each issue** using the mapping below.
4. **Propose a new milestone only if** the issue has a coherent theme that does not fit any existing milestone and is likely to attract more issues.
5. **Batch-assign by milestone**
   ```bash
   gh issue edit NUM1 NUM2 NUM3 --milestone "Exact Milestone Title"
   ```
6. **Verify nothing is left unassigned**
   ```bash
   gh issue list --state open --milestone none --limit 200
   ```

## Milestone Mapping for ailujsoi

| Theme | Milestone |
|---|---|
| AoT / `juliars --emit-binary` / pure-Rust backend | **AoT Backend Expansion (2026-07)** |
| Compiler, runtime, dispatch, subtyping, `JuliaType`, specialization | **Compiler and Runtime Modernization** |
| `main` regression, prevention, CI gates, product quality | **Product Quality and Platform Hardening** |
| Module scope, cache restore, binding resolution | **Module Scope and Cache-Restore Parity (2026-07)** |
| VM performance, typed-loop IR, broadcast kernel, runtime specializer codegen | **VM Performance and Typed-Loop Optimizations (2026-07)** |
| Iterators, generators, collections, `Memory`, array literal semantics | **Iteration, Generators, and Collection Semantics (2026-07)** |
| Parser, lexer, lowering, macro, syntax compatibility | **Parser, Lowering, and Syntax Compatibility Gaps (2026-07 follow-up)** |
| Regex, display, version string, text representation | **Display, Version, and Text Representation Parity (2026-07 follow-up)** |
| Rust refactoring, clippy, helper consolidation, orphaned code | **Rust Code Quality and Helper Consolidation** |
| Pure Julia base/stdlib implementation | **Pure Julia 化（継続）** |
| Architecture debt, upstream Julia structural parity audit | **アーキテクチャ負債・本家 Julia 構造パリティ監査 (2026-07)** |
| REPL, module evaluation state, session lifecycle | **REPL and Module Evaluation State (2026-07)** |
| Register VM work | **RegisterVM** |
| Compile-time speed | **CompileSpeed** |
| Missing / three-valued logic parity | **Missing and Three-Valued Logic Parity** |

## Tie-Break Rules

| Conflict | Prefer |
|---|---|
| Multiple milestones could fit | The **most specific** milestone (narrow scope beats broad scope). |
| Performance vs. feature work | The milestone that names the **primary intent** of the issue. |
| Documentation vs. UX text | Documentation if it changes reference/help content; Frontend/UX if it changes in-app strings. |
| Testing/CI/coverage improvements | Quality & Reliability or DevEx if it exists; otherwise Tech Debt only if it is maintenance. |
| Infrastructure vs. backend | Backend if it changes application logic; a new milestone only if it is observability/security/DevEx/quality & reliability. |

## New Milestone Checklist

Before creating a new milestone, confirm:

- [ ] The issue does not fit any existing milestone even with loose interpretation.
- [ ] The category has a clear, short title (2–5 words).
- [ ] You can describe it in one sentence.
- [ ] At least one other open or upcoming issue plausibly belongs there.

**Good names:** "Observability & Monitoring", "Quality & Reliability", "Developer Experience".

## Common Mistakes

- **Creating a milestone for one issue.** A single issue should usually fit an existing bucket.
- **Ignoring milestone descriptions.** Read descriptions; titles can be misleading.
- **Using broad catch-all milestones.** Route to the actual subsystem or quality area.
- **Not verifying.** Always re-run the unassigned list after batch edits.
