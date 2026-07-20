#!/usr/bin/env bash
# Build a STATIC ARMv6 `sjulia` binary for the original Raspberry Pi Zero /
# Zero W (ARM1176 / ARMv6) via Docker, ready to `scp` to the device.
#
# The Pi Zero 1 is ARMv6; Debian `armhf` (ARMv7-baseline) binaries SIGILL on it.
# This cross-compiles for `arm-unknown-linux-musleabihf` (ARMv6, musl, static)
# inside docker/Dockerfile.pizero-armv6, so the output runs on any Pi Zero OS
# regardless of glibc version. On arm64 hosts (Apple Silicon) the build is
# native-speed — no QEMU.
#
# This script only *builds and places* the binary on the host (default:
# dist/pizero/sjulia). Copy it to the Pi yourself, e.g.:
#   scp dist/pizero/sjulia pi@raspberrypi.local:~/
#
# Usage:
#   scripts/build_sjulia_pizero.sh [options]
#
# Options:
#   --dest DIR          Output directory for the binary (default: dist/pizero)
#   --self-test         Smoke-run the built binary under QEMU (docker) after build.
#   --no-cache          docker build --no-cache (force a clean rebuild).
#   --image REF         Override the musl-cross toolchain image.
#   -h, --help          Show this help.
#
# Examples:
#   scripts/build_sjulia_pizero.sh
#   scripts/build_sjulia_pizero.sh --self-test --dest build/pi
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DOCKERFILE="docker/Dockerfile.pizero-armv6"
DEST_DIR="dist/pizero"
SELF_TEST=0
NO_CACHE=""
IMAGE_ARG=()

usage() { sed -n '2,34p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

while [ $# -gt 0 ]; do
  case "$1" in
    --dest)      DEST_DIR="$2"; shift 2 ;;
    --self-test) SELF_TEST=1; shift ;;
    --no-cache)  NO_CACHE="--no-cache"; shift ;;
    --image)     IMAGE_ARG=(--build-arg "MUSL_IMAGE=$2"); shift 2 ;;
    -h|--help)   usage 0 ;;
    *) echo "error: unknown argument: $1" >&2; usage 1 ;;
  esac
done

command -v docker >/dev/null 2>&1 || { echo "error: docker not found on PATH" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "error: Docker daemon is not running (start Docker Desktop)" >&2; exit 1; }

BIN="${DEST_DIR}/sjulia"
echo ">> Building static ARMv6 sjulia for Raspberry Pi Zero 1 ..."
echo "   Dockerfile: ${DOCKERFILE}"
echo "   Output:     ${BIN}"

# buildx exports the `export` stage's filesystem (just /sjulia) to DEST_DIR.
docker buildx build \
  -f "${DOCKERFILE}" \
  --target export \
  ${NO_CACHE} \
  ${IMAGE_ARG[@]+"${IMAGE_ARG[@]}"} \
  --output "type=local,dest=${DEST_DIR}" \
  .

[ -f "${BIN}" ] || { echo "error: expected ${BIN} was not produced" >&2; exit 1; }
chmod +x "${BIN}"

echo ">> Built binary:"
file "${BIN}"
ls -lh "${BIN}" | awk '{print "   size:", $5}'

# Verify it is what we expect: 32-bit ARM, statically linked. Fail loudly if not
# — a dynamically-linked or wrong-arch binary would not run on the Pi Zero.
DESC="$(file -b "${BIN}")"
case "${DESC}" in
  *ARM*) : ;;
  *) echo "error: built binary is not ARM: ${DESC}" >&2; exit 1 ;;
esac
case "${DESC}" in
  *statically\ linked*) : ;;
  *) echo "warning: binary is not reported as statically linked — it may fail on the Pi if its glibc differs" >&2 ;;
esac

if [ "${SELF_TEST}" -eq 1 ]; then
  echo ">> Self-test under QEMU (docker linux/arm/v6) ..."
  docker run --rm --platform linux/arm/v6 \
    -v "$(cd "${DEST_DIR}" && pwd)":/out \
    arm32v6/busybox:latest /out/sjulia -e 'println("pizero-armv6 ok: ", 1 + 2)' \
    || { echo "error: self-test failed to run the binary under QEMU" >&2; exit 1; }
fi

BIN_ABS="$(cd "$(dirname "${BIN}")" && pwd)/$(basename "${BIN}")"
echo ""
echo "Done. Binary placed at:"
echo "  ${BIN_ABS}"
echo ""
echo "Copy it to your Pi Zero yourself, e.g.:"
echo "  scp ${BIN} pi@raspberrypi.local:~/"
echo "  ssh pi@raspberrypi.local"
echo "  ./sjulia -e 'println(1 + 2)'"
echo "  ./sjulia mandelbrot.jl   # (copy examples/mandelbrot.jl over too)"
echo ""
echo "Note: the prelude + Base caches are embedded in the binary, so it starts"
echo "fast on the Pi Zero without compiling Base from source."
