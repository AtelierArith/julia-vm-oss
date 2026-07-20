# Nightly scope fixture-parity coverage design (#11693)

## Problem

The fixture upstream-parity sweep already detects fixtures that pass sjulia but
fail upstream Julia, but its nightly category list omits `scope`. The two
fixtures corrected by #11599 lived in that category, so the prevention lane did
not cover the failure family that motivated it.

## Design

Add `scope` to the explicit audited-category invocation in
`.github/workflows/nightly-gates.yml`. Keep the list explicit rather than using
`--all` because the broader rollout and its drift allowlist remain tracked by
#10246.

Extend the existing harness contract test to parse the workflow's backslash-
continued sweep command with shell tokenization and require `scope`. This binds
the prevention to the executable nightly command instead of a comment or an
unreferenced category list.

## Verification

Run the focused contract test and its registered source audit, then execute
`bash scripts/check_fixture_parity_sweep.sh --jobs 8 scope` against release
sjulia and upstream Julia. Any current divergence must be recorded in the
two-sided allowlist; a clean category needs no allowlist change.

