#!/usr/bin/env bash
# Self-test for scripts/docs_doctest.sh (Issue #8720).

set -euo pipefail

RUNNER="scripts/docs_doctest.sh"

if [[ ! -x "$RUNNER" ]]; then
    echo "ERROR: $RUNNER not found or not executable." >&2
    exit 2
fi

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

cat > "$tmpdir/fake_sjulia" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
script="$1"
if grep -q '1 + 2' "$script"; then
    echo 3
elif grep -q 'println("hello")' "$script"; then
    echo hello
else
    echo "unexpected script" >&2
    cat "$script" >&2
    exit 9
fi
EOF
chmod +x "$tmpdir/fake_sjulia"

cat > "$tmpdir/sample.md" <<'EOF'
# Sample

```julia-doctest
println(1 + 2)
# output
3
```

```julia-doctest
println("hello")
# output
hello
```
EOF

SJULIA_BIN="$tmpdir/fake_sjulia" DOCS_DOCTEST_SKIP_UPSTREAM=1 \
    bash "$RUNNER" "$tmpdir/sample.md" > "$tmpdir/pass.out"
if ! grep -q "OK: 2 julia-doctest block(s) passed" "$tmpdir/pass.out"; then
    echo "FAIL: expected two doctest blocks to pass" >&2
    cat "$tmpdir/pass.out" >&2
    exit 1
fi

cat > "$tmpdir/fail.md" <<'EOF'
```julia-doctest
println(1 + 2)
# output
4
```
EOF

if SJULIA_BIN="$tmpdir/fake_sjulia" DOCS_DOCTEST_SKIP_UPSTREAM=1 \
    bash "$RUNNER" "$tmpdir/fail.md" > "$tmpdir/fail.out" 2>&1; then
    echo "FAIL: doctest runner accepted mismatched output" >&2
    cat "$tmpdir/fail.out" >&2
    exit 1
fi
if ! grep -q "expected" "$tmpdir/fail.out"; then
    echo "FAIL: mismatch output should include expected/actual context" >&2
    cat "$tmpdir/fail.out" >&2
    exit 1
fi

echo "OK: scripts/docs_doctest.sh self-tests pass (Issue #8720)."
