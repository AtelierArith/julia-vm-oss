# Git / PR Flow

Regular merge, never squash.

## Branch & commit

```bash
git branch --show-current            # confirm where you are FIRST (shared worktrees get switched)
git checkout main && git pull
git checkout -b feat/your-feature
git add <files> && git commit -m "..."   # named files only; never `git add .`
git push -u origin your-branch
```

## Pre-PR gates (Rust touched)

```bash
cargo fmt --check                              # clippy passing ≠ fmt-clean
rustfmt --edition 2021 <only files you touched>   # if the check fails
cargo clippy --all-targets -- -D warnings
```

## Create the PR

```bash
gh pr create --draft --title "..." --body "$(cat <<'EOF'
## Summary
- <bullet points>

## Test plan
- [ ] <checklist of TODOs for testing the pull request...>

Linked Issue: #NNNN
EOF
)"
```

Reference the Issue number you created in the workaround/unsupported-feature
workflow. Link related Issues in the PR body.

## Merge

```bash
bash scripts/premerge_gate.sh --pr <N>   # lead: certify, mark ready, regular merge
```

Keep every agent-created implementation PR draft until review and all required
local gates finish. Implementation agents never mark ready or merge; the lead's
exact-main/exact-head guarded certification owns that transition (Issue #11056).

## Post-PR updates (Issue #1812)

1. Update `docs/vm/DONE.md`, `UNIMPLEMENTED.md`, `STATUS.md`.
2. Dated-header policy (Issue #3760): group new entries under a shared
   date-bearing daily `## ...YYYY-MM-DD...` header, with each issue as its own
   `### ... (Issue #NNNN)` subsection. If today's header already exists, add a
   subsection under it instead of prepending a new top-level "latest" block or
   rewriting older entries.
3. Yearly archive policy (Issue #6341): keep only the recent (~3 months,
   ≤3,000 lines) dated sections in `STATUS.md` / `DONE.md`. When the year
   changes (or a file exceeds 3,000 lines), move older dated sections verbatim
   to `docs/vm/archive/STATUS-<YYYY>.md` / `docs/vm/archive/DONE-<YYYY>.md`
   (mechanical cut & paste, no rewriting), upstream Julia NEWS/HISTORY style.
4. After base/ changes, verify exports:
   ```bash
   cargo nextest run --test fixture_tests base_exports_do_not_exceed_upstream
   ```
5. If new Clippy patterns were introduced, update Code Audits (Issue #3292).
6. Run the post-mortem (`sjulia-postmortem`): memory entry + prevention /
   follow-up Issues, then report completion.

## Required builds before merging (when touching VM/compiler/runtime)

```bash
cargo build --release -p subset_julia_vm --bin sjulia --features repl
cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios
cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios-sim
# Web (if touched):
wasm-pack build --target web --profile web-release
# iOS app (if touched):
xcodebuild -project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj \
  -scheme SubsetJuliaVMApp -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,name=iPad (A16)' build
```
