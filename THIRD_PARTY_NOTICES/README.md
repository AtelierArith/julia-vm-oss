# Third Party Notices

This directory contains license and notice files for:

- external Rust crates that are direct dependencies of the workspace crates
- bundled Julia packages listed in
  `subset_julia_vm/src/julia/packages/mod.rs`

Rust crate dependencies were resolved with:

```sh
cargo metadata --format-version 1 --locked --all-features
```

Local path dependencies within this repository are excluded. Transitive
dependencies are not included here. Rust crate notices are stored directly under
this directory as `<crate>-<version>/`.

The files were copied from the exact crates.io package sources selected by
`Cargo.lock`, rather than from upstream repository default branches, so the
notices match the resolved crate versions. GitHub CLI 2.95.0 also supports
single-file retrieval with `gh repo read-file`, for example:

```sh
gh repo read-file LICENSE --repo OWNER/REPO --ref TAG --output LICENSE
```

Bundled Julia package notices are stored under `julia-packages/`. Their upstream
repositories were identified from Julia General registry `Package.toml` entries,
and license files were downloaded with `gh repo read-file` from the matching
`v<version>` tag where available. Any exception is documented in that package's
`METADATA.txt`.
