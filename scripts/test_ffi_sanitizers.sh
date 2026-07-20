#!/usr/bin/env bash
# Build the native C ABI with ASan and run a C harness with ASan/UBSan (Issue #9004).
set -euo pipefail
cd "$(dirname "$0")/.."

skip_if_unavailable=0
if [[ "${1:-}" == "--skip-if-unavailable" ]]; then
  skip_if_unavailable=1
fi

toolchain="${SANITIZER_TOOLCHAIN:-nightly}"
host="$(rustc -vV | sed -nE 's/^host: (.*)$/\1/p')"
target="${SANITIZER_TARGET:-$host}"
cc_bin="${CC:-clang}"
out_dir="${SANITIZER_OUT_DIR:-target/ffi-sanitizers}"

skip_or_fail() {
  if [[ "${skip_if_unavailable}" -eq 1 ]]; then
    echo "SKIP: $*"
    exit 0
  fi
  echo "ERROR: $*" >&2
  exit 2
}

cargo +"${toolchain}" --version >/dev/null 2>&1 \
  || skip_or_fail "cargo +${toolchain} is unavailable"
cargo +"${toolchain}" -Z help >/dev/null 2>&1 \
  || skip_or_fail "cargo +${toolchain} does not accept -Z flags"
command -v "${cc_bin}" >/dev/null 2>&1 \
  || skip_or_fail "C compiler '${cc_bin}' is unavailable"

if [[ "${target}" == *apple-darwin* && "${SANITIZER_ALLOW_DARWIN:-0}" != "1" ]]; then
  skip_or_fail "macOS ASan interposition is not reliable for this cdylib harness; run on Linux CI or set SANITIZER_ALLOW_DARWIN=1 to experiment locally"
fi

mkdir -p "${out_dir}"

RUSTFLAGS="-Zsanitizer=address -Cforce-frame-pointers=yes ${RUSTFLAGS:-}" \
  cargo +"${toolchain}" build \
    -Z build-std \
    --target "${target}" \
    --release \
    -p subset_julia_vm_ffi \
    --features ffi-panic-test

lib_dir="target/${target}/release"
case "${target}" in
  *apple-darwin*) dylib="${lib_dir}/libsubset_julia_vm.dylib" ;;
  *linux*) dylib="${lib_dir}/libsubset_julia_vm.so" ;;
  *) skip_or_fail "unsupported sanitizer target '${target}'" ;;
esac

if [[ ! -f "${dylib}" ]]; then
  echo "ERROR: sanitizer build did not produce ${dylib}" >&2
  exit 1
fi

"${cc_bin}" \
  -std=c11 \
  -g \
  -O1 \
  -fno-omit-frame-pointer \
  -fsanitize=address,undefined \
  -DSJULIA_FFI_PANIC_TEST \
  -I subset_julia_vm_ffi/include \
  subset_julia_vm_ffi/tests/ffi_sanitizer_smoke.c \
  -L "${lib_dir}" \
  -lsubset_julia_vm \
  -Wl,-rpath,"$(pwd)/${lib_dir}" \
  -o "${out_dir}/ffi_sanitizer_smoke"

default_asan_options="detect_leaks=0:strict_string_checks=1:abort_on_error=1"
export ASAN_OPTIONS="${ASAN_OPTIONS:-${default_asan_options}}"
export UBSAN_OPTIONS="${UBSAN_OPTIONS:-print_stacktrace=1:halt_on_error=1}"
ld_library_path="$(pwd)/${lib_dir}${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
dyld_library_path="$(pwd)/${lib_dir}${DYLD_LIBRARY_PATH:+:${DYLD_LIBRARY_PATH}}"
export LD_LIBRARY_PATH="${ld_library_path}"
export DYLD_LIBRARY_PATH="${dyld_library_path}"

if [[ "${target}" == *apple-darwin* && -z "${DYLD_INSERT_LIBRARIES:-}" ]]; then
  asan_dylib="$("${cc_bin}" -print-file-name=libclang_rt.asan_osx_dynamic.dylib)"
  if [[ ! -f "${asan_dylib}" ]]; then
    skip_or_fail "could not locate macOS ASan runtime dylib via ${cc_bin}"
  fi
  export DYLD_INSERT_LIBRARIES="${asan_dylib}"
fi

"${out_dir}/ffi_sanitizer_smoke"
