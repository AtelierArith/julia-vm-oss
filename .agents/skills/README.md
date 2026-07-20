# Agent Skills — canonical location

This directory is the **single canonical home** for all project-scoped Agent
Skills. `.claude/skills` and `.cursor/skills` are symlinks to it, so Claude
Code and Cursor discover every skill natively; agents without built-in skill
support (Codex, opencode, Gemini, …) reach them through the dispatch table in
`AGENTS.md` (match the trigger → read the `SKILL.md` → follow it).

## Layout

```
.agents/skills/<name>/SKILL.md    # required; optional companion .md files next to it
```

`SKILL.md` frontmatter:

- `name` (required) — must equal the directory name
- `description` (required) — the trigger text agents match against; write it
  as "Use when …" with the concrete situations that should activate the skill
- `allowed-tools` (optional, Claude Code only) — other agents ignore it

## Authoring conventions (keep skills robust on smaller models)

Skills are executed by agents of varying capability (Sonnet-class included).
Write them so the weakest expected executor still gets them right:

- **`description` = trigger only.** "Use when …" + concrete symptoms. NEVER
  summarize the workflow in the description — agents will follow the summary
  instead of reading the body.
- **Hard rules first.** Non-negotiables go in a `## Hard rules` (or 禁止事項)
  block near the top, before the workflow.
- **Numbered steps with copy-paste commands.** Every step an agent must run is
  a concrete command, not a description of one. Key decisions are tables keyed
  to observable predicates, not prose.
- **Checklist + red flags for discipline skills.** End rule-enforcing skills
  with a completion checklist and a "Red flags — STOP" list of the exact
  rationalizations that precede violations.
- **Keep SKILL.md focused (~600 words when possible).** Heavy reference goes
  in a companion `.md` next to it, with an explicit "your task involves X →
  read Y now" pointer in SKILL.md.
- **No stale facts.** Commands, paths, and script names must exist in the repo
  today; re-verify when editing (a dead path silently derails smaller models).
- **Quote YAML descriptions containing `#`.** In an unquoted frontmatter
  scalar, ` #` starts a YAML comment and silently truncates the trigger text
  (e.g. `… "issue #123" …` cuts at `"issue`). Wrap the whole description in
  quotes.

## Conventions

- **Add here, never under `.claude/` or `.cursor/` directly** — those are
  symlinks; a real file there would silently fork the set.
- **Cross-reference other skills by bare name** (e.g. `sjulia-report-gap`),
  not by `@name` or an agent-specific path — every agent can resolve a name
  against this directory.
- **Reference the guide as `AGENTS.md`** (CLAUDE.md / GEMINI.md are symlinks
  to it).
- If a skill depends on an agent-specific capability (e.g. the Claude Code
  `Workflow` tool in `fix-bug-issues`), say so explicitly and provide a
  fallback path for agents without it.
- After adding or renaming a skill, update the skills table in `AGENTS.md`
  (and `REPOSITORY_RULES.md` if the skill encodes a durable rule).

## Current skills

See the "Agent Skills" table in `AGENTS.md` for the authoritative list and
trigger summaries.
