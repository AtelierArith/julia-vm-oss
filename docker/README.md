# Docker Builds

Compatibility checks for non-standard environments. All commands run from the
**repository root** unless stated otherwise.

| Dockerfile | Environment | Platform |
|---|---|---|
| `Dockerfile.raspberrypi32` | 32-bit Raspberry Pi OS (armhf) | `linux/arm/v7` |
| `Dockerfile.termux` | Android Termux userland | host arch (x86\_64 / arm64) |

---

## Prerequisites

**Docker 20+** with BuildKit enabled (Docker Desktop ships this by default).

For `linux/arm/v7` emulation on non-ARM hosts (Apple Silicon, x86\_64), register
QEMU binary format handlers once per boot:

```bash
docker run --privileged --rm tonistiigi/binfmt --install arm
```

Verify emulation is active:

```bash
docker run --rm --platform linux/arm/v7 arm32v7/debian:bookworm-slim uname -m
# expected: armv7l
```

---

## 32-bit Raspberry Pi (`Dockerfile.raspberrypi32`)

Verifies that sjulia builds and runs correctly on 32-bit Raspberry Pi OS
(`armv7-unknown-linux-gnueabihf`). Key invariants: `Sys.WORD_SIZE == 32`,
`Int === Int32`, `UInt === UInt32`.

### Build targets

| Target | What it does | Approx. time (QEMU) |
|---|---|---|
| `smoke` | Release build + precompile-cache embed + smoke assert | ~50–90 min |
| `nextest` | Release build + `cargo nextest run --release --lib` | several hours |

### Quick host-side cross-compile check (no Docker, no execution)

Before waiting for the full QEMU build, verify the VM library type-checks for
the 32-bit target on the host:

```bash
rustup target add armv7-unknown-linux-gnueabihf
timeout 1800 cargo check -p subset_julia_vm --lib --target armv7-unknown-linux-gnueabihf
```

This catches type errors and API mismatches in seconds without any emulation.

### Smoke build (recommended)

Builds sjulia in release mode, embeds precompile caches, then asserts basic
runtime invariants:

```bash
docker run --privileged --rm tonistiigi/binfmt --install arm

docker buildx build \
  --platform linux/arm/v7 \
  -f docker/Dockerfile.raspberrypi32 \
  --target smoke \
  .
```

**What the smoke target verifies:**

```
dpkg --print-architecture  →  armhf
rustc -Vv                  →  host: armv7-unknown-linux-gnueabihf
file target/release/sjulia →  ELF 32-bit LSB pie executable, ARM
sjulia -e '...'            →  Sys.WORD_SIZE == 32
                              Int === Int32
                              UInt === UInt32
                              println(1 + 2)  →  3
```

**Cache embedding:** The smoke target performs a two-stage build to avoid
recompiling Base from source on every `sjulia` invocation:

1. Build `sjulia` (initial binary, ~45 min under QEMU).
2. Run `--precompile-prelude` and `--precompile-base` to write cache files.
3. Rebuild `sjulia` with `SJULIA_PRELUDE_PROGRAM_CACHE` /
   `SJULIA_BASE_CACHE` set — this embeds the caches into the binary.

Without step 3 the cold-start `sjulia -e` assertion took ~167 s under QEMU
because Base was compiled from source on every run.

### Nextest gate (heavier)

Runs the full unit-test suite under QEMU. Very slow — prefer the smoke check
for routine validation:

```bash
docker buildx build \
  --platform linux/arm/v7 \
  -f docker/Dockerfile.raspberrypi32 \
  --target nextest \
  .
```

### Tips

- Add `--progress=plain` to see per-layer output instead of a compact summary.
- Add `--no-cache` to force a clean rebuild (e.g., after a major Cargo.lock
  change).
- The base image is `arm32v7/rust:1.96-bookworm`. Pin a different version via
  `--build-arg RUST_IMAGE=arm32v7/rust:X.Y-bookworm`.
- Layer caching means a re-run after a small source change only recompiles the
  VM, not the dependencies.

---

## Termux (`Dockerfile.termux`)

Verifies that the VM library type-checks inside the Termux Android userland
(`*-linux-android` toolchain). No QEMU required — runs on the host architecture.

### Build targets

| Target | What it does |
|---|---|
| `check` | `cargo check -p subset_julia_vm --lib` inside Termux |

### Check build

```bash
docker buildx build \
  -f docker/Dockerfile.termux \
  --target check \
  .
```

**What the check target verifies:**

```
rustc -Vv   →  host: *-linux-android  (e.g. x86_64-linux-android)
cargo -V    →  version line
cargo check →  subset_julia_vm::lib compiles under Termux toolchain
```

This is a type-check only — it does not build or run `sjulia`.

---

## Recommended workflow

```
1. cargo check --target armv7-unknown-linux-gnueabihf   # seconds, no Docker
2. docker buildx build … --target smoke                 # ~1 h, full armhf run
3. docker buildx build … --target nextest               # hours, full test suite
```

Start with step 1 for fast feedback, escalate to step 2 before merging a PR
that touches platform-sensitive code (numeric types, `usize`/pointer-size
assumptions, `Sys.WORD_SIZE`).
