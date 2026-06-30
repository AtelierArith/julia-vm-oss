# Test: Complex array Rust fast-path ↔ pure-Julia fallback parity (Issue #7876, P2)
#
# sjulia keeps Complex arrays in an interleaved `[re0, im0, re1, im1, ...]` Rust
# representation with dedicated fast paths for broadcast / matmul / reductions
# (docs/COMPARISION.md "乖離②"; RUST_BOUNDARY_JUSTIFICATION.md condition 4 — the
# no-JIT performance boundary). This is a deliberate performance tradeoff, NOT a
# semantic one: the fast path must produce *exactly* what ordinary scalar
# Complex multiple dispatch (the pure-Julia "gold standard") produces.
#
# This fixture guarantees that fallback equivalence holds. For each fast-path
# operation it recomputes the same result element-by-element through scalar
# Complex arithmetic — which goes through ordinary multiple dispatch, NOT the
# interleaved Rust fast path — and asserts equality. If a future representation
# change makes the fast path diverge from the scalar path, this breaks.
#
# Verified to pass identically under upstream `julia` (parity gold standard).

using Test

@testset "Complex array fast-path <-> pure-Julia fallback parity (#7876)" begin
    a = [Complex(1.0, 2.0), Complex(3.0, -4.0), Complex(-5.0, 6.0)]
    b = [Complex(0.5, 1.5), Complex(-2.0, 2.0), Complex(7.0, -1.0)]

    # --- broadcast add ---
    fast_add = a .+ b
    gold_add = [a[i] + b[i] for i in 1:length(a)]
    @test fast_add == gold_add

    # --- broadcast sub ---
    fast_sub = a .- b
    gold_sub = [a[i] - b[i] for i in 1:length(a)]
    @test fast_sub == gold_sub

    # --- broadcast mul ---
    fast_mul = a .* b
    gold_mul = [a[i] * b[i] for i in 1:length(a)]
    @test fast_mul == gold_mul

    # --- broadcast scalar mul (scalar Complex * array) ---
    s = Complex(2.0, -1.0)
    fast_smul = s .* a
    gold_smul = [s * a[i] for i in 1:length(a)]
    @test fast_smul == gold_smul

    # --- abs over array (transcendental fast path) ---
    fast_abs = abs.(a)
    gold_abs = [abs(a[i]) for i in 1:length(a)]
    @test fast_abs == gold_abs

    # --- conj over array ---
    fast_conj = conj.(a)
    gold_conj = [conj(a[i]) for i in 1:length(a)]
    @test fast_conj == gold_conj

    # --- real / imag projection ---
    @test real.(a) == [real(a[i]) for i in 1:length(a)]
    @test imag.(a) == [imag(a[i]) for i in 1:length(a)]

    # --- reduction (sum folds through the array) ---
    fast_sum = sum(a)
    gold_sum = a[1] + a[2] + a[3]
    @test fast_sum == gold_sum

    # --- matmul (complex matrix * vector) ---
    M = [Complex(1.0, 1.0) Complex(2.0, 0.0); Complex(0.0, -1.0) Complex(3.0, 2.0)]
    v = [Complex(1.0, 0.0), Complex(0.0, 1.0)]
    fast_mv = M * v
    gold_mv = [M[1, 1] * v[1] + M[1, 2] * v[2],
               M[2, 1] * v[1] + M[2, 2] * v[2]]
    @test fast_mv == gold_mv

    # --- matmul (complex matrix * matrix) ---
    N = [Complex(2.0, 0.0) Complex(0.0, 1.0); Complex(1.0, -1.0) Complex(0.0, 0.0)]
    fast_mm = M * N
    gold_mm = [M[1, 1] * N[1, 1] + M[1, 2] * N[2, 1]  M[1, 1] * N[1, 2] + M[1, 2] * N[2, 2];
               M[2, 1] * N[1, 1] + M[2, 2] * N[2, 1]  M[2, 1] * N[1, 2] + M[2, 2] * N[2, 2]]
    @test fast_mm == gold_mm
end

true
