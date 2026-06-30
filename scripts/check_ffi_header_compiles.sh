#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HEADER="$ROOT_DIR/subset_julia_vm_ffi/include/subset_vm.h"
SRC_DIR="$ROOT_DIR/subset_julia_vm_ffi/src"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/sjulia-ffi-header.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

missing=0

while IFS= read -r fn_name; do
  if ! grep -Eq "[[:space:]]${fn_name}[[:space:]]*\\(" "$HEADER"; then
    echo "ERROR: exported FFI function is missing from subset_vm.h: $fn_name" >&2
    missing=1
  fi
done < <(
  python3 - "$SRC_DIR" <<'PY'
import pathlib
import re
import sys

src_dir = pathlib.Path(sys.argv[1])
pattern = re.compile(
    r"#\[no_mangle\]\s*pub\s+extern\s+\"C\"\s+fn\s+([A-Za-z_][A-Za-z0-9_]*)",
    re.MULTILINE,
)
names = sorted({match.group(1) for path in src_dir.glob("*.rs") for match in pattern.finditer(path.read_text())})
for name in names:
    print(name)
PY
)

if [[ "$missing" -ne 0 ]]; then
  exit 1
fi

cat > "$TMP_DIR/consumer.c" <<'C'
#include <stddef.h>
#include "subset_vm.h"

static void stream_callback(void* context, const char* output) {
    (void)context;
    (void)output;
}

int main(void) {
    CExecutionResult* result = compile_and_run_streaming("1 + 1", 42, NULL, stream_callback);
    const char* value_json = execution_result_value_json(result);
    CValueKind kind = execution_result_value_kind(result);
    double real = execution_result_complex_real(result);
    double imag = execution_result_complex_imag(result);
    uint64_t len = execution_result_array_len(result);
    CValueKind element_kind = execution_result_array_element_kind(result, 0);
    double first = execution_result_array_element_f64(result, 0);
    char* element_json = execution_result_array_element_json(result, 0);
    uint64_t dict_len = execution_result_dict_len(result);
    char* key_json = execution_result_dict_key_json(result, 0);
    char* val_json = execution_result_dict_value_json(result, 0);
    const char* artifact_mime = execution_result_artifact_mime(result);
    const char* artifact_data = execution_result_artifact_data(result);
    (void)value_json;
    (void)kind;
    (void)real;
    (void)imag;
    (void)len;
    (void)element_kind;
    (void)first;
    (void)dict_len;
    (void)artifact_mime;
    (void)artifact_data;
    free_string(element_json);
    free_string(key_json);
    free_string(val_json);
    free_execution_result(result);
    subset_julia_vm_demo();
    return 0;
}
C

cat > "$TMP_DIR/consumer.cpp" <<'CPP'
#include <cstddef>
#include "subset_vm.h"

static void stream_callback(void* context, const char* output) {
    (void)context;
    (void)output;
}

int main() {
    CExecutionResult* result = compile_and_run_detailed("complex(1.0, 2.0)", 42);
    CValueKind kind = execution_result_value_kind(result);
    const char* json = execution_result_value_json(result);
    const char* mime = execution_result_artifact_mime(result);
    (void)kind;
    (void)json;
    (void)mime;
    free_execution_result(result);
    result = compile_and_run_streaming("1 + 1", 42, nullptr, stream_callback);
    free_execution_result(result);
    return 0;
}
CPP

cc -std=c11 -I"$ROOT_DIR/subset_julia_vm_ffi/include" -fsyntax-only "$TMP_DIR/consumer.c"
c++ -std=c++17 -I"$ROOT_DIR/subset_julia_vm_ffi/include" -fsyntax-only "$TMP_DIR/consumer.cpp"

echo "OK: subset_vm.h covers exported FFI functions and compiles as C/C++ (Issue #8455)"
