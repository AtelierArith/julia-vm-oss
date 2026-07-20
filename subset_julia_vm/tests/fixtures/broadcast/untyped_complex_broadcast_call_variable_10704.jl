using Test

# Issue #10704: `f.(A, B)` where `f` is an UNTYPED user function and BOTH `A`
# and `B` are arrays.
#
# `Base.Broadcast`'s bulk typed-kernel fast path (`_try_broadcast_typed_kernel`,
# Issues #8797/#9693) only fires for broadcasts with EXACTLY ONE array argument
# (the `f.(A, scalar)` shape the Mandelbrot acceptance benchmark uses); its
# `array_arg.is_some() { return None }` guard bails as soon as a second array
# argument appears. A two-array `f.(A, B)` broadcast therefore falls back to
# `Base.Broadcast`'s generic elementwise loop, which calls `f` through a
# *function value* stored in a local (`Instr::CallFunctionVariable`) once per
# element — a materially different per-element call path from the one the
# acceptance benchmark exercises, and one that previously had no ComplexF64
# coverage at all. This fixture pins that path's correctness.
#
# (Investigating #10704 established that the acceptance benchmark's own shape
# never reaches `Instr::CallFunctionVariable`: the bulk kernel intercepts it
# first. The residual typed-vs-untyped performance gap was root-caused to a
# peephole-fusion parity gap in the runtime specializer's ComplexF64 codegen
# and is tracked separately as Issue #10799.)

# Returns Int64 from two ComplexF64 arguments — no type annotations, so the
# runtime specializer (not the static compiler) produces the body that runs.
# Written with an if/return (not a ternary `? :`): the runtime specializer does
# not compile a ternary conditional, so a ternary-bodied callee would fall back
# to the generic interpreter and not exercise the specialized path.
function classify(a, b)
    if abs2(a) > abs2(b)
        return 1
    end
    return 0
end

# Returns ComplexF64 from two ComplexF64 arguments — exercises the mixed-type
# `TypedScalarFunctionBlock` frame-less executor (not the pure-I64/F64 ones).
add_pair(a, b) = a + b

@testset "untyped f.(A, B) two-array broadcast: CallFunctionVariable fast path" begin
    A = ComplexF64[1.0+1.0im 2.0+0.5im; -1.0+0.5im 0.0+0.0im]
    B = ComplexF64[0.5+0.5im 1.0+1.0im; 0.5-0.5im 2.0+2.0im]

    counts = classify.(A, B)
    @test counts == [1 1; 1 0]
    @test sum(counts) == 3

    sums = add_pair.(A, B)
    @test sums == ComplexF64[1.5+1.5im 3.0+1.5im; -0.5+0.0im 2.0+2.0im]
end

@testset "untyped f.(A, scalar) single-array broadcast still correct" begin
    # Exercises the pre-existing bulk typed-kernel path (single array arg);
    # pinned alongside the two-array case so both call shapes stay covered.
    mandelbrot_escape(c, maxiter) = begin
        z = 0.0 + 0.0im
        for k in 1:maxiter
            abs2(z) > 4.0 && return k - 1
            z = z * z + c
        end
        maxiter
    end
    C = ComplexF64[0.0+0.0im 1.0+1.0im; -1.0+0.5im 0.5+0.0im]
    counts = mandelbrot_escape.(C, 10)
    @test counts == [10 2; 5 5]
    @test sum(counts) == 22
end

@testset "non-concrete element type falls back correctly" begin
    # `Any`-typed array (declared/static element type is non-concrete, even
    # though every actual element happens to be Int64 here): the runtime
    # specializer cannot specialize a single body from the array's own
    # (non-concrete) declared element type, so this must fall back to the
    # generic dispatch path and still produce the correct (upstream-matching)
    # result — same values, kept as a single concrete type throughout so this
    # fixture stays independent of the (separately tracked, Issue #10787)
    # per-element promoted-type-preservation gap for TRULY mixed-type `Any`
    # arrays.
    mixed = Any[1, 2, 3, 4]
    double(x) = x + x
    result = double.(mixed)
    @test result == [2, 4, 6, 8]
    @test all(x -> x isa Int64, result)
end

@testset "NaN / Inf / signed-zero complex elements" begin
    classify2(a, b) = abs2(a) > abs2(b) ? 1 : 0
    A = ComplexF64[NaN+0.0im Inf+0.0im; -0.0+0.0im 0.0+(-0.0)im]
    B = ComplexF64[0.0+0.0im 0.0+0.0im; 0.0+0.0im 0.0+0.0im]
    counts = classify2.(A, B)
    # abs2(NaN + 0im) is NaN; NaN > 0 is false in both Julia and IEEE 754.
    @test counts[1, 1] == 0
    # abs2(Inf + 0im) is Inf; Inf > 0 is true.
    @test counts[1, 2] == 1
    # -0.0 and +0.0 compare equal; abs2(-0.0 + 0.0im) == abs2(0.0 + 0.0im) == 0.0.
    @test counts[2, 1] == 0
    @test counts[2, 2] == 0

    add2(a, b) = a + b
    sums = add2.(A, B)
    @test isnan(real(sums[1, 1]))
    @test isinf(real(sums[1, 2]))
    @test sums[2, 1] == -0.0 + 0.0im
    @test sums[2, 2] == 0.0 + 0.0im
end

@testset "keyword-parameter callee through a function-value variable" begin
    # Trap guard for any future `Instr::CallFunctionVariable` fast path
    # (Issue #10704): a call through a function-value variable carries NO
    # keyword arguments at the call site, but the resolved callee can still
    # declare a required keyword parameter. Any shortcut that skips the
    # normal frame setup would also skip `bind_kwargs_defaults`'s
    # `UndefKeywordError` check and default-value evaluation, silently
    # returning a value where upstream Julia raises. An adversarial review
    # of a candidate fast path for this instruction hit exactly that bug, so
    # the invariant is pinned here even though that fast path was not merged.
    function f(x; y)
        return x
    end
    g = f
    @test_throws UndefKeywordError g(1)
    @test g(1; y=2) == 1
end

true
