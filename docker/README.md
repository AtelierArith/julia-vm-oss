# Docker Builds

Compatibility checks for non-standard environments. All commands run from the
**repository root** unless stated otherwise.

| Dockerfile | Environment | Platform |
|---|---|---|
| `Dockerfile.pizero-armv6` | **Ship** a static `sjulia` to the original Pi Zero / Zero W (ARM1176 / ARMv6) | cross → `arm-unknown-linux-musleabihf` |
| `Dockerfile.raspberrypi32` | Compatibility *check* on 32-bit Raspberry Pi OS (armhf) | `linux/arm/v7` |
| `Dockerfile.termux` | Android Termux userland | host arch (x86\_64 / arm64) |

> **Pi Zero 1 vs Zero 2 W:** the original Pi Zero / Zero W is **ARMv6**
> (BCM2835 / ARM1176); use `Dockerfile.pizero-armv6`. The Pi Zero **2 W** is
> ARMv7-capable (Cortex-A53) and uses the ARMv7 `Dockerfile.raspberrypi32`. An
> ARMv7 binary will `SIGILL` on an ARMv6 Pi Zero 1.

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

## Raspberry Pi Zero 1 — ship a static binary (`Dockerfile.pizero-armv6`)

Builds a **fully static** `sjulia` for the original Raspberry Pi Zero / Zero W
(BCM2835 / ARM1176JZF-S — ARMv6 + VFPv2) and places it on the host so you can
`scp` it to the device. Unlike the `raspberrypi32` *compatibility check*, this is
a *deliverable*: cross-compiled for `arm-unknown-linux-musleabihf` (ARMv6, musl,
`crt-static`), so it runs on any Pi Zero OS regardless of glibc version.

```bash
scripts/build_sjulia_pizero.sh                 # -> dist/pizero/sjulia
scripts/build_sjulia_pizero.sh --self-test     # also smoke-run it under QEMU
```

Then copy it over yourself:

```bash
scp dist/pizero/sjulia pi@raspberrypi.local:~/
ssh pi@raspberrypi.local
./sjulia -e 'println(1 + 2)'
```

Key points:

- **arm64-native, no QEMU for the build.** The `messense/rust-musl-cross`
  toolchain image is arm64, so on Apple Silicon the cross-compile runs at native
  speed (~10–15 min cold, seconds warm).
- **ARM1176 codegen pin.** The Dockerfile sets
  `-C target-cpu=arm1176jzf-s` so Rust emits ARMv6 / VFPv2 / Thumb-1. The build
  fails if the ELF is ever `Tag_CPU_arch: v7+` (that would `SIGILL` on the Pi
  Zero 1). The binary is often still *tagged* `Tag_FP_arch: VFPv3` because the
  precompiled `rust-std` carries that attribute — this is a conservative label,
  not real VFPv3 instruction use: the binary runs correctly on an emulated
  ARM1176 core (`qemu-arm -cpu arm1176`, verified).
- **Embedded prelude + Base cache.** The build runs the cross-built ARMv6 binary
  under qemu to generate the caches, then rebuilds with `SJULIA_*_CACHE` set so
  they are baked into the shipped binary (`&'static [u8]`). The Pi Zero's first
  run starts fast instead of compiling Base from source on a single ARMv6 core.
- **Rigorous local check** (optional, emulates the real core):

  ```bash
  docker run --rm -v "$PWD/dist/pizero":/out arm64v8/debian:bookworm bash -c \
    'apt-get update -qq && apt-get install -y -qq qemu-user >/dev/null && \
     qemu-arm -cpu arm1176 /out/sjulia -e "println(sqrt(2.0))"'
  ```

## 32-bit Raspberry Pi (`Dockerfile.raspberrypi32`)

Verifies that sjulia builds and runs correctly on 32-bit Raspberry Pi OS
(`armv7-unknown-linux-gnueabihf`). Key invariants: `Sys.WORD_SIZE == 32`
(from the host `usize::BITS`), but `Int === Int64` and `UInt === UInt64` —
sjulia keeps a **uniform 64-bit integer model** on every target, so `Int` /
`UInt` stay 64-bit even where the host word size is 32-bit (Issue #7310).

### Build targets

| Target | What it does | Approx. time (QEMU) |
|---|---|---|
| `example` | Single-stage release build + run `examples/mandelbrot.jl` | ~45–55 min |
| `smoke` | Release build + precompile-cache embed + smoke assert + Mandelbrot run | ~50–90 min |
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
                              Int === Int64      # uniform 64-bit model (Issue #7310)
                              UInt === UInt64
                              println(1 + 2)  →  3
sjulia examples/mandelbrot.jl →  ASCII Mandelbrot (broadcast + ComplexF64 + Ref)
```

**Cache embedding:** The smoke target performs a two-stage build to avoid
recompiling Base from source on every `sjulia` invocation:

1. Build `sjulia` (initial binary, ~45 min under QEMU).
2. Run `--precompile-prelude` and `--precompile-base` to write cache files.
3. Rebuild `sjulia` with `SJULIA_PRELUDE_PROGRAM_CACHE` /
   `SJULIA_BASE_CACHE` set — this embeds the caches into the binary.

Without step 3 the cold-start `sjulia -e` assertion took ~167 s under QEMU
because Base was compiled from source on every run.

### Fast example run (`--target example`)

The quickest way to confirm sjulia actually *runs* on 32-bit ARM. Single-stage
build (no precompile-cache embed), then it runs `examples/mandelbrot.jl` — a
broadcast + `ComplexF64` + `Ref` kernel that prints an ASCII Mandelbrot. Roughly
half the wall time of `smoke`; the trade-off is that Base compiles from source on
the one run (a few minutes under QEMU).

```bash
docker run --privileged --rm tonistiigi/binfmt --install arm

docker buildx build \
  --platform linux/arm/v7 \
  -f docker/Dockerfile.raspberrypi32 \
  --target example \
  --progress=plain \
  .
```

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
