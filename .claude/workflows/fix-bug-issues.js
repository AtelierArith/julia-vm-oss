export const meta = {
  name: 'fix-bug-issues',
  description: 'Triage, fix, draft PR, and verify open bug-labeled GitHub issues for SubsetJuliaVM; lead certification merges separately',
  whenToUse: 'When you want to clear the open `bug` label backlog: clusters issues by root cause, fixes each cluster in an isolated git worktree with a fixture test, opens one draft PR per work-item, and adversarially verifies each fix against upstream julia for later lead certification.',
  phases: [
    { title: 'Triage', detail: 'Read every bug issue and cluster by genuine shared root cause into work-items' },
    { title: 'Fix', detail: 'One isolated git worktree per work-item: reproduce, fix, add fixture test, targeted tests, push branch, open PR' },
    { title: 'Verify', detail: 'Adversarially review each PR diff against upstream julia as the oracle' },
  ],
}

// ---- Inputs -------------------------------------------------------------
// `args.issues` (REQUIRED) is the live list of open bug-issue numbers, collected by
// the caller via `gh issue list --label bug --state open`. We deliberately do NOT
// hardcode a snapshot: a stale snapshot caused a run to "fix" already-closed issues.
const REPO = 'AtelierArith/ailujsoi'
const PRIMARY_DIR = '/Users/terasaki/work/atelierarith/ailujsoi'
// `args` may arrive as an object, as a JSON-encoded string (depending on how the
// workflow is invoked), or as a bare array of issue numbers — normalize all three.
let input = args
if (typeof input === 'string') {
  try { input = JSON.parse(input) } catch (_) { input = {} }
}
if (Array.isArray(input)) input = { issues: input }
input = input || {}
log(`args received: type=${typeof args}; normalized.issues=${JSON.stringify(input.issues)}`)
const ISSUES = Array.isArray(input.issues) ? input.issues : []

if (!ISSUES.length) {
  log('No issues provided. Pass args.issues — collect the LIVE list first with:')
  log(`  gh issue list --repo ${REPO} --label bug --state open --json number --jq '[.[].number]'`)
  return { error: 'no issues provided; pass args.issues (live open bug-issue numbers)' }
}

// ---- Schemas ------------------------------------------------------------
const TRIAGE_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    workItems: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        properties: {
          id: { type: 'string', description: 'short kebab slug, e.g. "free-var-regression"' },
          issues: { type: 'array', items: { type: 'integer' }, description: 'issue numbers in this work-item' },
          title: { type: 'string' },
          rootCause: { type: 'string', description: 'one-paragraph root-cause hypothesis' },
          suggestedBranch: { type: 'string', description: 'e.g. "fix/free-var-regression-7618"' },
          likelyFiles: { type: 'array', items: { type: 'string' } },
          brief: { type: 'string', description: 'concrete instructions for the fixer' },
          priority: { type: 'string', enum: ['urgent-main-red', 'normal'] },
        },
        required: ['id', 'issues', 'title', 'rootCause', 'suggestedBranch', 'brief', 'priority'],
      },
    },
    notes: { type: 'string', description: 'cross-cutting notes, ordering/dependency hints' },
  },
  required: ['workItems', 'notes'],
}

const FIX_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    id: { type: 'string' },
    issues: { type: 'array', items: { type: 'integer' } },
    status: { type: 'string', enum: ['fixed-pr', 'fixed-no-pr', 'partial', 'blocked'] },
    branch: { type: 'string' },
    prUrl: { type: 'string' },
    filesChanged: { type: 'array', items: { type: 'string' } },
    testsAdded: { type: 'array', items: { type: 'string' } },
    reproBefore: { type: 'string', description: 'sjulia output BEFORE fix vs upstream julia (proves the bug)' },
    targetedTestResult: { type: 'string', description: 'pass/fail summary of the targeted tests run' },
    clippyResult: { type: 'string' },
    summary: { type: 'string' },
    blockers: { type: 'string' },
  },
  required: ['id', 'issues', 'status', 'summary'],
}

const VERIFY_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    id: { type: 'string' },
    verdict: { type: 'string', enum: ['confirmed', 'concerns', 'rejected'] },
    matchesUpstream: { type: 'boolean', description: 'does the added test expected output match upstream julia?' },
    addressesRootCause: { type: 'boolean', description: 'real fix vs symptom mask' },
    testActuallyCovers: { type: 'boolean' },
    regressionRisk: { type: 'string' },
    concerns: { type: 'string' },
  },
  required: ['id', 'verdict', 'matchesUpstream', 'addressesRootCause', 'testActuallyCovers', 'concerns'],
}

// ---- Prompts ------------------------------------------------------------
const triagePrompt = `You are triaging open \`bug\`-labeled GitHub issues for SubsetJuliaVM (a static no-JIT subset-of-Julia VM; repo ${REPO}). Working dir is the repo root — read ./CLAUDE.md and ./docs/vm/ as needed.

Issues to triage: ${ISSUES.map(n => '#' + n).join(', ')}.

For EACH issue run \`gh issue view <n> --repo ${REPO}\` (and \`gh issue view <n> --comments\` when useful) to read the full body, MWE, and any julia-vs-sjulia output table. Use \`codegraph_explore\` / reading source under subset_julia_vm/ to confirm where the bug lives.

Cluster issues into WORK-ITEMS by GENUINE shared root cause or shared edit surface ONLY:
- Group issues together ONLY when fixing one would touch/conflict with the other, or they are literally the same regression (same file + same edit surface). Read the issues to decide — do not group by topic area alone.
- Do NOT over-cluster: independent bugs that merely live in the same area ("macro", "metaprogramming", "dispatch") but have different root causes must stay SEPARATE so each PR stays small and reviewable.
- This repo is under heavy PARALLEL development; some issues may ALREADY be fixed on current \`main\`. The fixer will reproduce on up-to-date origin/main first and skip the ones that no longer reproduce — your job is just clustering.
- Mark a work-item priority "urgent-main-red" ONLY if you can confirm it makes \`main\` red right now (a build break, or a failing default-suite test on a clean checkout).

For each work-item provide: a short slug id, the issue numbers, a title, a one-paragraph rootCause hypothesis, a suggestedBranch (fix/<slug>-<lowest-issue#>), likelyFiles, a concrete brief for the fixer, and priority.

Return ONLY the structured object.`

function fixPrompt(item) {
  return `You are fixing a bug (work-item "${item.id}") in SubsetJuliaVM, a strict static subset-of-Julia VM (no JIT). You are in a FRESH, ISOLATED git worktree of repo ${REPO} — your edits cannot affect other agents. Working dir is the worktree root; read ./CLAUDE.md FIRST and follow it exactly (design principles, fixture conventions, error spans, "don't reset others' work").

WORK-ITEM
  issues:      ${item.issues.map(n => '#' + n).join(', ')}
  title:       ${item.title}
  root cause:  ${item.rootCause}
  brief:       ${item.brief}
  likelyFiles: ${(item.likelyFiles || []).join(', ') || '(discover)'}
  branch:      ${item.suggestedBranch}

STEPS
1. Run \`gh issue view <n> --repo ${REPO}\` for each issue above to get the full body + MWE + the julia-vs-sjulia output table.
2. REPRODUCE FIRST on up-to-date main. Build the VM once: \`cargo build --release --bin sjulia --features repl\` (required to relink after pure-Julia base/ changes). Run each MWE through \`./target/release/sjulia\` AND through upstream \`julia\` (1.12, on PATH). Confirm sjulia is wrong and capture julia's correct output — that julia output is your ground-truth fixture expected. Record this in reproBefore.
   - If the bug NO LONGER reproduces (already fixed on main by parallel work), set status "fixed-no-pr", explain in summary, and STOP — do NOT open a duplicate PR. Optionally still add a regression fixture if one is missing (then status "fixed-pr").
   - If you genuinely cannot reproduce for another reason, set status "blocked" and STOP.
3. Find the root cause. Prefer PURE-JULIA fixes in subset_julia_vm/src/julia/ that mirror upstream paths; prefer multiple dispatch over type-checks; centralize feature/lowering checks in lowering. Consult upstream Julia source for the canonical implementation — if ./julia is empty in this worktree (submodule not populated), read it from ${PRIMARY_DIR}/julia. Watch the promote-fallback recursion trap for numeric ops (CLAUDE.md).
4. Implement the minimal correct fix. Keep the diff focused on THIS work-item only; do not touch unrelated files; never \`git checkout\`/\`stash\` files you did not modify.
5. Add a regression test. For runtime/behavior bugs add a fixture under subset_julia_vm/tests/fixtures/<category>/ with a manifest.toml [[tests]] entry — verify its \`expected\` against upstream \`julia\` FIRST, and prefix the test name with its category (Issue #3135). For parser/lowering bugs add the appropriate parser/unit test. Re-run to confirm it now PASSES with your fix and FAILED without it.
6. Run ONLY targeted tests (full suite is 30 min — do NOT run it): e.g. \`timeout 1800 cargo nextest run --release --test fixture_tests <category>::\`, plus relevant \`--lib\` tests if you changed Rust. If you changed Rust, run \`cargo clippy\` scoped to the crate you touched. For AoT-pipeline changes also run \`bash scripts/test_aot.sh\` (the default suite does not build \`--features aot\`).
   IMPORTANT: If you observe a test/build failure that is clearly PRE-EXISTING and UNRELATED to your work-item (it also fails on a clean origin/main and your diff does not touch its surface), IGNORE it — do not let it block you and do not "fix" it here. But if your own change could plausibly have caused it, treat it as yours.
7. Open the PR. Create your branch off the up-to-date remote main:
     git fetch origin main --quiet && git switch -c ${item.suggestedBranch} origin/main
   Then \`git add\` your files (stage ONLY your intended files) and commit. End the commit message body with:
     Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
   Push: \`git push -u origin ${item.suggestedBranch}\`. Then \`gh pr create --draft --repo ${REPO} --base main\` with a title and a body that: lists "Closes #<n>" for each issue, summarizes the root cause + fix, gives a Test Plan (commands run + results), and ENDS with:
     🤖 Generated with [Claude Code](https://claude.com/claude-code)
   Do NOT enable auto-merge and do NOT merge (the lead handles certification and merging). Capture the PR URL.
8. If the fix is genuinely too large/risky/uncertain, set status "partial" or "blocked" with a clear blockers note rather than shipping a broken or speculative PR.

Return ONLY the structured object (status, branch, prUrl, filesChanged, testsAdded, reproBefore, targetedTestResult, clippyResult, summary, blockers).`
}

function verifyPrompt(fix, item) {
  return `Adversarially verify a bug-fix PR for SubsetJuliaVM. Be skeptical: your job is to find reasons the fix is WRONG, incomplete, or masks the symptom. Default to "concerns" if you cannot positively confirm. You are READ-ONLY: do not edit, commit, push, or merge anything.

WORK-ITEM "${item.id}" — issues ${item.issues.map(n => '#' + n).join(', ')}
Fix status: ${fix.status}
Branch:     ${fix.branch || '(none)'}
PR:         ${fix.prUrl || '(none)'}
Fixer summary: ${fix.summary}
${fix.blockers ? 'Blockers noted: ' + fix.blockers : ''}

If status is "blocked"/"partial"/"fixed-no-pr" with no PR, just report verdict "concerns" summarizing whether the reasoning is sound. Otherwise:
1. Fetch and read the diff: \`git fetch origin ${fix.branch || ''} --quiet\` then \`git --no-pager diff origin/main...origin/${fix.branch || ''}\` (or \`gh pr diff ${fix.prUrl || ''}\`).
2. ORACLE CHECK: run each issue's MWE through upstream \`julia\` (1.12, on PATH) to get the ground-truth output. Confirm the added fixture test's \`expected\` matches upstream julia EXACTLY (set matchesUpstream).
3. Read the diff critically: does it address the stated root cause or only mask the visible symptom (addressesRootCause)? Does the added test actually exercise the bug — would it fail on old code (testActuallyCovers)? Does it follow CLAUDE.md (pure-Julia-first, dispatch over type-checks, precise error spans, category-prefixed fixture names)?
4. Assess regression risk: broad/structural changes (e.g. to free-variable analysis or dispatch) carry higher risk — call it out (regressionRisk). Note anything the integrator must re-test in the FULL suite.
5. Set verdict "confirmed" ONLY if the fix is correct, matches upstream, and the test genuinely covers the bug with acceptable regression risk. Use "concerns" for anything uncertain, "rejected" if the fix is wrong or masks the symptom. All implementation PRs remain draft; only the lead's guarded certification may mark a confirmed PR ready and merge it (Issue #11056).

Return ONLY the structured object.`
}

// ---- Orchestration ------------------------------------------------------
phase('Triage')
log(`Triaging ${ISSUES.length} bug issues: ${ISSUES.map(n => '#' + n).join(', ')}`)
const triage = await agent(triagePrompt, { schema: TRIAGE_SCHEMA, phase: 'Triage', label: 'triage', effort: 'high' })

if (!triage || !triage.workItems || !triage.workItems.length) {
  log('Triage produced no work-items — aborting.')
  return { error: 'no work-items', triage }
}

// Urgent (main-red) work-items first so their fixes land at the front.
const items = triage.workItems.slice().sort((a, b) =>
  (a.priority === 'urgent-main-red' ? 0 : 1) - (b.priority === 'urgent-main-red' ? 0 : 1))

log(`Triage -> ${items.length} work-items: ${items.map(i => i.id + '[' + i.issues.join(',') + ']').join('  ')}`)

// Each implementation work-item flows independently through Fix -> Verify.
// Confirmed draft PRs are returned to the lead; this workflow never marks
// ready or merges (Issue #11056).
const results = await pipeline(
  items,
  item => agent(fixPrompt(item), {
    schema: FIX_SCHEMA,
    phase: 'Fix',
    label: 'fix:' + item.id,
    isolation: 'worktree',
  }),
  (fix, item) => {
    if (!fix) return { fix: null, verify: null, item }
    return agent(verifyPrompt(fix, item), {
      schema: VERIFY_SCHEMA,
      phase: 'Verify',
      label: 'verify:' + item.id,
      effort: 'high',
    }).then(verify => ({ fix, verify, item }))
  },
)

const clean = results.filter(Boolean)
return {
  triageNotes: triage.notes,
  workItemCount: items.length,
  results: clean.map(r => ({
    id: r.item.id,
    issues: r.item.issues,
    priority: r.item.priority,
    status: r.fix && r.fix.status,
    prUrl: r.fix && r.fix.prUrl,
    branch: r.fix && r.fix.branch,
    summary: r.fix && r.fix.summary,
    blockers: r.fix && r.fix.blockers,
    verdict: r.verify && r.verify.verdict,
    matchesUpstream: r.verify && r.verify.matchesUpstream,
    addressesRootCause: r.verify && r.verify.addressesRootCause,
    concerns: r.verify && r.verify.concerns,
    regressionRisk: r.verify && r.verify.regressionRisk,
  })),
}
