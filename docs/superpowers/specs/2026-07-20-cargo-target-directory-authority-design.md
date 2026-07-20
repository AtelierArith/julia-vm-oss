# Cargo target-directory authority design (#11695)

## Problem

AoT, metamorphic, and fixture-parity harnesses currently derive release binary
paths directly from `CARGO_TARGET_DIR`, falling back to `<repo>/target`. Cargo
can also redirect artifacts through `.cargo/config.toml` `build.target-dir`.
Without the environment variable, the harnesses then execute a different path
from the one used by their own Cargo build commands.

## Design

Add one Bash 3.2-compatible resolver in `scripts/cargo_target_dir.sh`. It asks
`cargo metadata --format-version 1 --no-deps` for the effective absolute
`target_directory`, decoding the JSON with Python 3. If Cargo, Python, metadata,
or the field is unavailable, it normalizes `${CARGO_TARGET_DIR:-<repo>/target}`
against the repository root.

Every binary-consuming harness sources that helper, resolves the directory
before assigning `SJULIA_BIN` / `JULIARS_BIN`, and exports the resolved absolute
directory so later Cargo producer commands and child harnesses use the same
location. Explicit binary overrides remain authoritative.

## Verification

Extend `tests/test_aot_binary_path_contract.py` to require every inventoried
consumer to use the shared resolver. Exercise the helper against a temporary
minimal Cargo project whose `.cargo/config.toml` redirects `build.target-dir`,
and preserve the existing default, relative/absolute environment, and explicit
binary-override contracts.

