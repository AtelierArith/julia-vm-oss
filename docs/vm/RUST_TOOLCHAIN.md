# Rust Toolchain Contract

Issue #11253 defines two separate Rust compatibility obligations.

## Supported versions

- **MSRV: Rust 1.95.** Every workspace package inherits
  `workspace.package.rust-version = "1.95"`. Code must compile on that version;
  raising it is an explicit compatibility change.
- **Local reference: Rust 1.95.0.** `rust-toolchain.toml` pins the exact compiler
  and installs `clippy` plus `rustfmt`. Local merge certification and reproduced
  lint failures use this version unless `RUSTUP_TOOLCHAIN` explicitly overrides
  it.
- **Current stable: moving additional lane.** The CI lint job deliberately uses
  `dtolnay/rust-toolchain@stable`, sets `RUSTUP_TOOLCHAIN=stable` to override the
  checked-in reference pin, and runs the same lane registry. A failure there
  reports ecosystem/lint drift; it does not silently redefine the checked-in
  MSRV or the local reference version.

MSRV compatibility and Clippy cleanliness are distinct. `rust-version` tells
Cargo which language/library floor the packages support. Zero-warning Clippy
gates are run on the pinned reference version and again on current stable.

## Clippy lanes

`scripts/run_clippy_lanes.sh --list` is the executable registry. With no lane
arguments it runs every registered lane; a lane name runs only that lane.

| Lane | Scope | Features | Owner |
|---|---|---|---|
| `default` | every workspace member, all host targets | none | `scripts/premerge_gate.sh` and current-stable CI |
| `repl` | `subset_julia_vm`, all targets | `repl` | current-stable CI and explicit local registry run |
| `aot` | `subset_julia_vm`, all targets | `aot` | `scripts/test_aot.sh` and current-stable CI |
| `aot-cranelift` | `subset_julia_vm`, all targets | `aot,cranelift` | current-stable CI and explicit local registry run |
| generated AoT Rust | temporary generated Cargo project | generated program | step 8 of `scripts/test_aot.sh` |

The generated-Rust lane stays in `test_aot.sh` because it requires the `juliars`
binary built earlier in that gate. `scripts/check_rust_toolchain_contract.sh`
pins that command alongside the four workspace-owned lanes.

Target-only source has compile owners outside Clippy: `platform-builds.yml`
builds `subset_julia_vm_ffi` for iOS and `subset_julia_vm_web` for WASM. The
default Clippy lane still uses `--workspace`, so both crates' host-buildable
source is included instead of being omitted by workspace `default-members`.

## Reproducing lint drift

Capture the exact tool versions before changing source:

```text
rustc -Vv
cargo clippy -V
```

Then reproduce the affected lane explicitly, for example:

```bash
bash scripts/run_clippy_lanes.sh aot
RUSTUP_TOOLCHAIN=stable bash scripts/run_clippy_lanes.sh aot
```

Record both version outputs with the diagnostic. A warning observed only on a
newer stable toolchain is still fixed when structurally correct, but the report
must not describe it as an MSRV failure.

## Changing the contract

A toolchain bump must update `rust-toolchain.toml`, the workspace
`rust-version`, this document, and the expectations in
`scripts/check_rust_toolchain_contract.sh` together. Run the contract audit's
negative self-test and every registered Clippy lane before merging.
