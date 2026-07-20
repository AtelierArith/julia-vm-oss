---
name: sjulia-postmortem
description: >-
  Use when a piece of work in this repo is finished — a PR merged, a bug fixed,
  an investigation concluded, a refactor or doc task wrapped up — and you are
  about to report completion to the user. Also use when the user says
  "postmortem", "振り返り", or asks what was learned or what should happen next.
---

# Post-Mortem (run after finishing any task)

Turn what you just learned into durable artifacts BEFORE reporting completion.
Produce three outputs, in this order. Never skip a step silently — when a step
produces nothing, write that explicitly in your final report ("no durable
insight" / "no follow-ups").

## Output 1 — Insight entry in `./memory/` (always attempt)

Ask: what did I learn that is NOT obvious from the code, git history, or
`AGENTS.md`? A trap you fell into, a debugging/verification technique that
worked, a root-cause pattern, an upstream-Julia nuance, a wrong assumption you
had to correct.

- Nothing non-obvious → say so in the final report and go to Output 2.
- Before creating a file, check for an existing one covering the same fact and
  update it instead: `grep -ril "<keyword>" memory/`. Delete entries the
  session proved wrong.
- Otherwise write ONE file per fact — `memory/project/` for Issue-linked work
  notes, `memory/feedback/` for process lessons:

```markdown
---
name: <short-kebab-slug>
description: <one line — used to decide relevance during recall>
metadata:
  type: project   # user | feedback | project | reference
---

<The fact. Use absolute dates, link related notes as [[other-name]].>

**Why:** <what broke or was slow without this knowledge>

**How to apply:** <the concrete action to take next time>
```

- Add ONE line to `memory/MEMORY.md` under the matching section (index only,
  ≤200 chars): `- [Title](<type>/<file>.md) — <hook>`
- Commit the memory files as their own logical commit (or with the docs
  commit) so they are shared across sessions.

## Output 2 — Bug fix? File the prevention Issue

If the finished task fixed a `bug` (wrong output / crash / existing-error
class): apply `sjulia-bug-prevention` now. Minimum content: root cause, why
existing tests missed it, the regression test you added, blast radius
(same-shape call sites), and one prevention mechanism (audit script /
checklist entry / coverage test / fixture / lint) — filed as an Issue.

Not a bug fix → go to Output 3.

## Output 3 — Follow-up Issues for everything deferred

List every "next time / someone should / noticed but didn't touch" item from
the session: deferred cases, TODO comments you added, adjacent smells, missing
tests, doc gaps, performance observations. For each item:

1. **Dedup first:**
   `gh issue list --state all --search "<keywords>" --limit 20`.
   Already covered → reference it in your PR body instead. Do NOT
   `gh issue comment` on Issues you did not create (rejected by policy).
2. **julia-vs-sjulia gap?** → that is `sjulia-report-gap` territory
   (MWE + output table + `unsupported-feature`/`bug` label), not a plain
   follow-up.
3. Otherwise file it:

```bash
gh issue create --title "<imperative next step>" --body "$(cat <<'EOF'
## Context
<what work surfaced this; PR / Issue links>

## Proposal
<what to do next, concretely>

## Definition of done
- [ ] <observable outcome>
EOF
)"
```

No follow-ups → write "no follow-ups" explicitly in the final report.

## Final-report checklist (include in your completion message)

- [ ] memory: <files written / updated, or "no durable insight">
- [ ] prevention Issue: #NNNN (bug fixes only, else "n/a")
- [ ] follow-up Issues: #NNNN, … (or "none")

## Red flags — you are about to skip the post-mortem

- "The task is done and the user is waiting — I'll skip the write-up."
- "This lesson is obvious." — If it cost you more than 15 minutes, it was not.
- "I'll file the follow-ups later." — Later never comes. File now or state
  "none".
- "The insight is already in my head for next time." — Sessions don't share
  heads. `./memory/` is the only channel.

Related skills: `sjulia-bug-prevention` (Output 2), `sjulia-report-gap`
(gap-type follow-ups), `report-issue` (Issue body templates).
