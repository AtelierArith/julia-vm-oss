# Issue Workflow: unsupported-feature / bug

The project is **issue-driven**: create the GitHub Issue *before* adding a
workaround or fix, then reference the Issue number in workaround comments,
tests, and the PR.

## Decision rule

| Situation | Label |
|-----------|-------|
| Construct **runs in upstream `julia`** but sjulia **cannot run** it (parse error, "unsupported"/"not implemented" runtime error, MethodError on otherwise-valid syntax) | `unsupported-feature` |
| sjulia **runs but produces wrong output**, OR you hit an existing sjulia error / crash / compatibility gap / implementation blocker during work | `bug` |

If you find such a construct **incidentally** while doing something else, do
NOT route around it — file the Issue first, even if it is incidental to the
current task.

## MWE template for `unsupported-feature`

Include a minimal working example and a julia-vs-sjulia output table:

```markdown
## MWE

```julia
# minimal code that runs in upstream julia but errors in sjulia
```

## Output

| Interpreter | Result |
|-------------|--------|
| `julia`     | <expected output / pass> |
| `sjulia`    | <error message / parse error / MethodError> |
```

## Creating the Issue

```bash
gh issue create --title "<short description>" \
  --label "unsupported-feature" \
  --body "$(cat <<'EOF'
## MWE
...
EOF
)"
```

Use `--label "bug"` instead when sjulia runs but produces wrong output.

## Workaround management

After creating the Issue and before/while implementing the workaround:

1. Add a workaround comment referencing the Issue:
   - Rust: `// Workaround: ... (Issue #NNNN)`
   - Julia: `# Workaround: ... (Issue #NNNN)`
2. Add an entry to `docs/vm/WORKAROUNDS.md` (centralized list, Issue #2843).
   Full section + Summary Table format: `sjulia-document-workaround`.
3. Run both sync scripts:
   ```bash
   bash scripts/check_workarounds_documented.sh
   bash scripts/check_workarounds_sync.sh
   ```

## Removing a workaround

1. Delete the workaround comment.
2. Move the entry to "Resolved" in `docs/vm/WORKAROUNDS.md`.
3. Add a regression test.
4. Run both check scripts above.
