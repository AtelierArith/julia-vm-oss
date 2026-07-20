<!--
SubsetJuliaVM pull-request template (Issue #9129).
Fill every section. Delete a section only if it is genuinely N/A and say why.
Canon: REPOSITORY_RULES.md "sjulia Invariants" + "Git Workflow And Logical Commits".
-->

## Summary

<!-- What changed and WHY (not a restatement of the diff). -->

Closes #<!-- issue number; use "Closes #A" per issue so each auto-closes -->

## sjulia Invariants touched

<!--
Declare which of the five invariants this PR touches (REPOSITORY_RULES.md
"sjulia Invariants"). Check every one you touch and say how; leave the rest
unchecked. Weakening an invariant needs an explicit reason + a tracking Issue.
-->

- [ ] **1. True Subset** — upstream parity (NS-1/NS-2, fixture parity, numeric matrix)
- [ ] **2. Pure Julia First** — Rust semantics surface / boundary justification
- [ ] **3. Single-threaded / no-JIT / panic-free VM** — iOS/WASM viability
- [ ] **4. Measurement-driven decisions** — Performance Decision Protocol followed
- [ ] **5. Issue-Driven + Prevention** — workaround/deferral tracked; audits intact

How each checked invariant is affected: <!-- one line per checked box -->

## North Star impact (NS-1 … NS-7)

<!--
Expected effect on each North Star metric (docs/vm/NORTH_STAR.md). "none" is a
valid answer. NS-1 (parity) and NS-2 (corpus) are monotonic ratchets: a
downward change requires a stated reason + Issue here. If measured numbers
differ from this prediction, add an addendum before merge.
-->

- NS-1 parity / NS-2 corpus: <!-- none / ↑ / ↓ (reason + Issue if ↓) -->
- NS-4 bench / NS-5 cold-start: <!-- none / measured Δ (Performance Decision Protocol) -->
- NS-7 debt (workarounds / structural): <!-- none / +N (Issue) / −N -->

## Test plan

<!-- Commands run and their result. Fixtures verified against upstream julia. -->

- [ ] `timeout 1800 cargo nextest run --release` (full suite after VM/dispatch/inference changes or parallel merges — fixtures + `--lib` green ≠ full green)
- [ ] `cargo fmt --check` (touched files) + `cargo clippy --all-targets -- -D warnings`
- [ ] Fixture parity: `bash scripts/fixture_julia_parity.sh <fixture.jl>` (verified against upstream `julia` first)
- [ ] AoT gate `bash scripts/test_aot.sh` (only if AoT-touching)
- [ ] Audits green, incl. `bash scripts/check_audit_negative_selftest.sh` (if audit scripts / their source paths moved)

## Definition of Done (Issue #9129)

- [ ] Every acceptance criterion of the linked Issue re-verified **one item at a time** in the closing comment; unmet items handed off to a scoped follow-up Issue (honest deferral), not silently omitted.
- [ ] For a large parent/track Issue: the **original purpose** is judged achieved independently of child-Issue completion.
- [ ] **Rebase-audit:** rebased onto fresh `origin/main` and ran `git diff origin/main` to confirm no sibling code was silently dropped by the rebase.
- [ ] Merge is a **regular merge** (`gh pr merge --auto --merge`), and I will confirm it actually landed (an auto-merge reservation is not "done").

<!--
CI-workflow registration TODO (maintainer with `workflow` scope): none required
for this template file. New `check_*.sh` audits still need their ci.yml stanza
per docs/vm/CODE_AUDITS.md "Adding a New Audit Script".
-->

🤖 Generated with [Claude Code](https://claude.com/claude-code)
