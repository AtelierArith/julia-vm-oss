#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE' >&2
Usage:
  scripts/audit_native_boundary_ccall.sh [--write]

Scans upstream Julia's base/ and stdlib/ ccall sites and checks the generated
native-boundary policy ledger.

Set SJULIA_UPSTREAM_JULIA=/path/to/julia when the local julia/ submodule is not
checked out in this worktree.
USAGE
}

mode="check"
if [[ "${1:-}" == "--write" ]]; then
  mode="write"
elif [[ $# -gt 0 ]]; then
  usage
  exit 2
fi

repo_root="$(git rev-parse --show-toplevel)"
upstream="${SJULIA_UPSTREAM_JULIA:-$repo_root/julia}"
out="$repo_root/docs/vm/NATIVE_BOUNDARY_CCALL_LEDGER.tsv"

if [[ ! -d "$upstream/base" || ! -d "$upstream/stdlib" ]]; then
  cat >&2 <<EOF
ERROR: upstream Julia checkout not found at: $upstream

Initialize the julia submodule for this worktree, or run with:
  SJULIA_UPSTREAM_JULIA=/path/to/julia scripts/audit_native_boundary_ccall.sh
EOF
  exit 2
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

printf 'path\tline\tpolicy\tfamily\tissue\trationale\tccall_site\n' > "$tmp"

classify() {
  local rel="$1"
  local text="$2"
  local policy="A"
  local family="julia-runtime"
  local issue="#9570"
  local rationale="Curated VM intrinsic or pure-Rust host wrapper; no user-visible arbitrary ccall."

  case "$rel" in
    base/docs/*|base/docs/basedocs.jl*)
      policy="C"
      family="docs-example"
      rationale="Documentation examples are not a shipped sjulia native boundary."
      ;;
    base/c.jl*|base/threadcall.jl*)
      policy="C"
      family="user-native-boundary"
      rationale="User-visible ccall/threadcall syntax is unsupported by design under the shared WASM+iOS requirement."
      ;;
    base/mpfr.jl*)
      family="mpfr-bigfloat"
      issue="#9290"
      rationale="Current policy is pure-Rust astro-float/BigFloat surface; MPFR dual-build would require a separate ADR."
      ;;
    base/irrationals.jl*)
      family="mpfr-bigfloat"
      issue="#9290"
      rationale="MPFR-backed constant generation is mirrored through the shared VM numeric surface, not exposed as arbitrary ccall."
      ;;
    base/gmp.jl*)
      family="gmp-bigint"
      rationale="Current policy is pure-Rust BigInt implementation; no GMP dynamic/native boundary."
      ;;
    base/pcre.jl*)
      family="pcre2-regex"
      issue="#8992"
      rationale="Current policy is pure-Rust regex/fancy-regex coverage rather than PCRE2 dynamic calls."
      ;;
    base/math.jl*|base/floatfuncs.jl*|base/special/*)
      family="libm"
      rationale="Current policy is Rust/libm-compatible math in the shared VM surface; platform-native libm is not a user ccall boundary."
      ;;
    base/fastmath.jl*)
      family="llvm-intrinsic"
      rationale="LLVM intrinsic spelling is a compiler-owned intrinsic boundary, not a user-visible native call surface."
      ;;
    stdlib/Random/*|base/random.jl*)
      policy="C"
      family="random-dsfmt"
      issue="#8998"
      rationale="dSFMT bitstream parity is a documented permanent divergence; sjulia uses its own RNG surface."
      ;;
    stdlib/LibGit2/*)
      policy="C"
      family="libgit2"
      rationale="LibGit2/Pkg native boundary is outside the current subset; do not expose arbitrary package-manager ccall."
      ;;
    base/libdl.jl*|stdlib/Libdl/*)
      policy="C"
      family="dynamic-loader"
      rationale="Dynamic library loading is incompatible with the shared WASM+iOS requirement."
      ;;
    stdlib/Mmap/*)
      family="mmap"
      rationale="If supported, this must be a curated Rust host boundary, not arbitrary Julia ccall."
      ;;
    stdlib/Sockets/*|base/libuv.jl*|base/stream.jl*|base/iostream.jl*|base/file.jl*|base/filesystem.jl*|base/process.jl*|base/asyncevent.jl*|base/env.jl*|base/libc.jl*|base/loading.jl*|base/path.jl*|base/stat.jl*|base/sysinfo.jl*|base/task.jl*|base/util.jl*|stdlib/FileWatching/*|stdlib/REPL/src/LineEdit.jl*)
      family="os-libuv-io"
      rationale="If supported, this must be a curated Rust host/OS boundary with WASM+iOS fallbacks."
      ;;
    base/cmem.jl*|base/hashing.jl*|base/io.jl*|base/secretbuffer.jl*|base/strings/basic.jl*|base/strings/cstring.jl*|base/strings/search.jl*|base/strings/substring.jl*|stdlib/Serialization/*)
      family="string-memory"
      rationale="Memory/string primitives must be VM-owned or pure-Rust helpers, not arbitrary native calls."
      ;;
    base/strings/unicode.jl*|stdlib/Unicode/*)
      family="unicode-utf8proc"
      rationale="Unicode/utf8proc behavior must be mirrored by VM-owned or pure-Rust Unicode support, not arbitrary native calls."
      ;;
    stdlib/SharedArrays/*)
      policy="C"
      family="shared-memory"
      rationale="Shared-memory process/thread boundary is outside the single-threaded VM target."
      ;;
    stdlib/Profile/*|stdlib/InteractiveUtils/*)
      family="reflection-profiler"
      rationale="Reflection/profiling access must be VM-owned metadata, not arbitrary native calls."
      ;;
  esac

  if [[ "$text" == *"ccall(:jl_"* || "$text" == *"ccall((:jl_"* || "$text" == *"@ccall(jl_"* ]]; then
    policy="A"
    family="julia-runtime"
    rationale="Julia runtime primitive mirrored by VM/compiler metadata or a curated Rust intrinsic."
  fi

  if [[ "$text" == \#* ]]; then
    policy="C"
    family="docs-example"
    rationale="Commented/documentation-only native call text is not a shipped sjulia surface."
  fi

  if [[ "$rel" == */test/* || "$rel" == */test* ]]; then
    policy="C"
    family="upstream-test-only"
    rationale="Upstream test-only native call; not a shipped sjulia surface."
  fi

  printf '%s\t%s\t%s\t%s\n' "$policy" "$family" "$issue" "$rationale"
}

while IFS=: read -r file line text; do
  rel="${file#$upstream/}"
  text="${text//$'\t'/ }"
  text="${text#"${text%%[![:space:]]*}"}"
  IFS=$'\t' read -r policy family issue rationale < <(classify "$rel" "$text")
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$rel" "$line" "$policy" "$family" "$issue" "$rationale" "$text" >> "$tmp"
done < <(rg -n --no-heading 'ccall\(' "$upstream/base" "$upstream/stdlib" | sort -V)

if [[ "$mode" == "write" ]]; then
  mv "$tmp" "$out"
  trap - EXIT
  echo "Updated $out"
  exit 0
fi

if ! cmp -s "$tmp" "$out"; then
  echo "ERROR: native-boundary ccall ledger is stale." >&2
  echo "Regenerate with:" >&2
  echo "  SJULIA_UPSTREAM_JULIA=$upstream scripts/audit_native_boundary_ccall.sh --write" >&2
  diff -u "$out" "$tmp" | sed -n '1,120p' >&2 || true
  exit 1
fi

echo "OK: native-boundary ccall ledger is up to date."
