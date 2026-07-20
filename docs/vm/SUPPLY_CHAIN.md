# Supply Chain Policy — SubsetJuliaVM (Issue #9000)

This document records the supply chain security posture for the SubsetJuliaVM
project: automated advisory/license gates, the `extern/` parity oracle manifest,
and the vendored fork tracking policy for `vendor/astro-float-num`.

---

## 1. Automated Gates (cargo-audit / cargo-deny)

### What runs and where

| Check | Tool | CI job | Failure action |
|---|---|---|---|
| RUSTSEC advisories | `cargo audit` | `nightly-gates.yml :: supply-chain` | Creates a GitHub Issue via `notify-on-failure` |
| Advisories + licenses + bans + sources | `cargo deny check` | `nightly-gates.yml :: supply-chain` | Same |
| Vendor fork drift | `scripts/check_vendored_drift.sh` | `nightly-gates.yml :: supply-chain` | Same |
| Manifest consistency | `scripts/check_vendored_drift.sh` (local self-check) | Per-PR `audits` batch | PR fails |

**Why nightly, not per-PR?**
Advisories are published by the RustSec team independently of this repo's
code changes. Tying the advisory gate to PR pushes would create intermittent
red-PR noise when a new advisory appears between PR opening and CI run,
blocking unrelated work. Nightly cadence matches the `upstream-test-sweep`
and other slow-changing gates.

### Configuration: `deny.toml`

`deny.toml` at the repo root is the cargo-deny configuration file.

**Allowlisted advisories** (pre-existing at Issue #9000 adoption time):

| RUSTSEC ID | Crate | Type | WHY allowed | Issue/Action |
|---|---|---|---|---|
| RUSTSEC-2025-0141 | `bincode 1.3.3` | unmaintained | Direct dep for Base cache serialization (Issue #2929); advisory says "complete", no CVE | Migration tracked in Issue #9000 follow-up |
| RUSTSEC-2024-0436 | `paste` | unmaintained | Transitive compile-time proc-macro; no runtime surface | Clears when upstream proc-macro consumers migrate |
| RUSTSEC-2026-0190 | `anyhow` | unsound | Transitive dep; `downcast_mut` path not reachable in this project | Upstream fix expected; tracked in Issue #9000 |

**Adding a new allowlist entry:**
1. Check that the advisory is genuinely not exploitable in this codebase.
2. Add an `{ id = "RUSTSEC-XXXX-XXXX", reason = "..." }` entry to `deny.toml`
   with a WHY comment explaining the decision.
3. Add a row to the table above.
4. Commit both changes in the same PR.

**Removing an allowlist entry:**
Delete the `deny.toml` entry and the table row when the underlying crate is
updated or the project stops depending on it.

### License policy

All dependency licenses must be in the `allow` list in `deny.toml`.
Current allowed licenses: MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception,
Unlicense, BSD-2-Clause, Zlib, Unicode-3.0.

When a new dependency uses a different license:
1. Verify the license is OSI-approved and compatible with the project's
   MIT license.
2. Add it to `deny.toml [licenses] allow`.
3. Add a `THIRD_PARTY_NOTICES/<crate>-<version>/` directory with the license
   text (matching the existing `THIRD_PARTY_NOTICES/` pattern).
4. Document the addition here.

---

## 2. extern/ Parity Oracle Manifest

### What is extern/?

`extern/` contains Julia package source trees cloned from upstream GitHub
repositories. They are used as:
- **Parity oracles**: reference implementations for fixture tests (e.g.
  `julia --project=extern/Rotations.jl fixture.jl` for upstream behavior).
- **Port reference**: source to read when implementing a bundled package
  (e.g. reading `extern/Plots.jl/PlotsBase/src/recipes.jl` while porting).

`extern/` is `.gitignore`'d (too large; ~1.3 GB total) and is NOT tracked
by git. The pinned version of each package is recorded in:

```
extern/MANIFEST.tsv
```

### Reproducing extern/

```bash
# Populate all packages from MANIFEST
bash scripts/populate_extern.sh

# Populate a single package
bash scripts/populate_extern.sh Rotations.jl
```

After cloning, verify SHA matches and update MANIFEST.tsv:

```bash
git -C extern/Rotations.jl rev-parse HEAD
# → update commit_sha column in extern/MANIFEST.tsv
```

### MANIFEST.tsv format

```
name    version    upstream_url    commit_sha    fetch_date    notes
```

- `name`: directory name under `extern/` (e.g. `Rotations.jl`)
- `version`: upstream git tag (e.g. `v1.7.1`)
- `upstream_url`: HTTPS clone URL
- `commit_sha`: 40-char SHA; `UNVERIFIED` if not yet cloned locally
- `fetch_date`: ISO-8601 date of last refresh
- `notes`: free-form; cite the Issue/PR where this version was chosen

### Convention (REPOSITORY_RULES.md)

When a fixture references `extern/<Pkg>.jl/<path>`, the MANIFEST version is
the authoritative parity oracle. Any drift between a fixture's expected output
and upstream Julia must be investigated against THIS version, not an ad-hoc
local checkout.

When updating a fixture for a new upstream behavior:
1. Update `extern/MANIFEST.tsv` to the new version.
2. Run `bash scripts/populate_extern.sh <Pkg>.jl`.
3. Update the fixture expected output.
4. Commit MANIFEST.tsv + fixture together.

---

## 3. Vendored Fork Tracker

### What is vendored?

`vendor/astro-float-num/` is a locally-patched fork of `astro-float-num`
(upstream: https://github.com/stencillogic/astro-float). The fork is
wired into the workspace via `[patch.crates-io]` in `Cargo.toml`.

**Patches applied:**
- Issue #6794: Ziv precision-refinement loop bounding in `ops/pow.rs` and
  `ops/log.rs` — prevents hang on table-maker's-dilemma inputs (e.g.
  `big(4.0)^0.5`).
- Issue #6921: Silenced 5 compile warnings on rustc 1.83+ (dead increment,
  elided return lifetimes).

### Drift detection

`scripts/check_vendored_drift.sh` queries crates.io for the latest published
`astro-float-num` version and compares it to our pinned version in
`vendor/astro-float-num/Cargo.toml`. It runs in the nightly `supply-chain`
job. A drift finding means:

> Upstream released a new version — review it.

### Review process (quarterly, or on drift alert)

1. Check the upstream changelog:
   https://github.com/stencillogic/astro-float/releases
2. Classify the delta:
   - **Security/correctness fix**: re-apply the Issue #6794 patch to the new
     version. Update `vendor/astro-float-num/` and bump Cargo.toml version.
   - **No relevant fix**: record the review decision in this document's
     Vendored Fork Review Log (below) and update `next_review`.
   - **Upstream fixed the hang**: remove the fork entirely, delete
     `vendor/astro-float-num/`, remove the `[patch.crates-io]` entry, and
     add a "Resolved" entry in the Review Log.

### Vendored Fork Review Log

| Date | Upstream version checked | Our version | Action taken | Reviewer |
|---|---|---|---|---|
| 2026-07-03 | v0.3.6 | v0.3.6 | Initial adoption; no drift. Gate established. | Issue #9000 |

**Next scheduled review:** 2026-10-03 (quarterly)

### Re-patching procedure

When adopting a new upstream `astro-float-num` version:

```bash
# 1. Clone the new upstream version
git clone --branch vX.Y.Z https://github.com/stencillogic/astro-float /tmp/astro-fresh

# 2. Apply our patches (review the diff carefully for conflicts)
cd /tmp/astro-fresh
# Apply Issue #6794 patch: bound the Ziv loops in ops/pow.rs and ops/log.rs
# Apply Issue #6921 patch: fix rustc 1.83+ warnings

# 3. Replace vendor/astro-float-num/src/ with the patched source
cp -r src/ <repo>/vendor/astro-float-num/src/

# 4. Update vendor/astro-float-num/Cargo.toml version field
# 5. Run the full test suite to verify no regressions:
#    timeout 1800 cargo nextest run --release
# 6. Commit with message: "chore: update vendored astro-float-num to vX.Y.Z (Issue #6794)"
```

---

## 4. Third-Party Notices

`THIRD_PARTY_NOTICES/` contains license files for dependencies bundled in
iOS/WASM distributions. The cargo-deny `[licenses]` check mechanically
verifies that all dependency licenses are in our allowlist; the
`THIRD_PARTY_NOTICES/` directory is the human-readable artifact for
distribution compliance.

When adding a new direct dependency:
1. `cargo deny check licenses` must still pass.
2. Add a `THIRD_PARTY_NOTICES/<crate>-<version>/LICENSE` and
   `THIRD_PARTY_NOTICES/<crate>-<version>/METADATA.txt`.

---

## 5. Handling New RUSTSEC Advisories

When the nightly `supply-chain` job creates a failure issue:

1. **Assess exploitability**: does this project's code reach the vulnerable
   API path? Use `cargo deny check advisories` to see the dependency chain.
2. **If exploitable or high CVSS**: update the dependency immediately; open
   a dedicated fix PR.
3. **If not exploitable or low CVSS (unmaintained/unsound)**: add a
   time-bounded allowlist entry in `deny.toml` with a clear WHY and a
   tracking action. Add a row to § 1's allowlist table.
4. In either case, close the nightly failure issue once resolved (either
   by update or by documented allowlist entry).
