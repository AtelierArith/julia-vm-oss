# Integer comprehension / for-head range fast path: a runtime `Float` /
# `BigInt` BOUND that is non-finite (`NaN` / `±Inf`) or out of the `Int64`
# range raises the upstream `InexactError` when the range counts toward it,
# while the legal EMPTY direction stays error-free (Issue #9377, follow-up to
# #9321).
#
# Upstream rule (`base/range.jl`): for `start:step:stop`, let
# `q = (stop - start) / step`; if `q < 0` the range is empty (length 0, no
# error), otherwise `floor(Int, q)` is evaluated and raises `InexactError` for
# `NaN` / `±Inf` / `Int64`-overflow. Before the fix, the shared
# `CoerceRangeStopI64` instruction only saw `step` + `stop`, so it silently
# floored/ceiled a pathological bound into a `0`-length or `i64::MAX`-saturated
# range (`g(NaN) == 0`, `g(Inf)` looped ~`typemax(Int)` times) instead of
# raising. It now also receives `start` and mirrors the upstream `q` test; the
# raised `InexactError` is catchable (`try`/`catch` binds an `InexactError`).

using Test

@testset "Comprehension range bound InexactError (Issue #9377)" begin
    g(n) = length([i for i in 1:n])

    # Finite / empty-direction bounds keep the #9321 behavior (no error).
    @test g(5.5) == 5
    @test g(-3.0) == 0
    @test g(-Inf) == 0   # empty direction: NO error (upstream length(1:-Inf) == 0)
    @test g(5) == 5

    # Counting-direction pathological bounds raise a catchable InexactError.
    @test_throws InexactError g(NaN)
    @test_throws InexactError g(Inf)
    @test_throws InexactError g(1.0e30)
    @test_throws InexactError g(big"1000000000000000000000000000000")

    # Descending explicit step: the directions flip.
    f(m) = length([i for i in 10:-2:m])
    @test f(2.5) == 4
    @test f(100.0) == 0
    @test f(Inf) == 0    # empty direction for a negative step: NO error
    @test_throws InexactError f(-Inf)
    @test_throws InexactError f(NaN)
    @test_throws InexactError f(-1.0e30)

    # Out-of-Int64 BigInt bound in the EMPTY direction: length 0, no error.
    @test length([i for i in 1:big"-1000000000000000000000000000000"]) == 0
    @test length([i for i in 10:-2:big"1000000000000000000000000000000"]) == 0
end

@testset "Cartesian / flatten range bound InexactError (Issue #9377)" begin
    # The single-var, cartesian, and flatten comprehension arms share the same
    # bound coercion; cover the pathological bound in each clause position.
    mc(n) = length([i + j for i in 1:n, j in 1:2])
    @test mc(5.5) == 10
    @test mc(-Inf) == 0
    @test_throws InexactError mc(NaN)
    @test_throws InexactError mc(1.0e30)
    mc_second(n) = length([i + j for i in 1:2, j in 1:n])
    @test mc_second(3.5) == 6
    @test_throws InexactError mc_second(NaN)

    fl(n) = length([i + j for i in 1:n for j in 1:2])
    @test fl(5.5) == 10
    @test fl(-Inf) == 0
    @test_throws InexactError fl(NaN)
    @test_throws InexactError fl(1.0e30)
    fli(n) = length([i + j for i in 1:2 for j in 1:n])
    @test fli(3.5) == 6
    @test_throws InexactError fli(NaN)
end

@testset "for-head range bound InexactError (Issue #9377)" begin
    # for-head fast path with an Any-inferred bound (ternary join of F64/I64
    # keeps the bound `Any` even under argument-type specialization).
    function hc(v, flag)
        c = 0
        n = flag ? v : 6
        for i in 1:n
            c += 1
        end
        c
    end
    @test hc(5.5, true) == 5
    @test hc(-3.0, true) == 0
    @test hc(-Inf, true) == 0   # empty direction: NO error
    @test hc(0.0, false) == 6   # integer bound unchanged
    @test_throws InexactError hc(NaN, true)
    @test_throws InexactError hc(Inf, true)   # pre-fix: ~typemax(Int) iterations
    @test_throws InexactError hc(1.0e30, true)

    # Constant NEGATIVE step for-head (const-step fast path).
    function hd(v, flag)
        c = 0
        n = flag ? v : 2
        for i in 10:-2:n
            c += 1
        end
        c
    end
    @test hd(2.5, true) == 4
    @test hd(Inf, true) == 0    # empty direction for a negative step: NO error
    @test_throws InexactError hd(-Inf, true)
    @test_throws InexactError hd(NaN, true)
    @test_throws InexactError hd(-1.0e30, true)

    # RUNTIME variable step (dynamic-step for-head path). The step is an
    # I64-inferred ternary so the loop keeps the integer fast path (an
    # `Any`-inferred step diverts to the generic ForEach range per #9291).
    function hv(v, flag, up)
        c = 0
        n = flag ? v : 2
        k = up ? 2 : -2
        for i in 1:k:n
            c += 1
        end
        c
    end
    @test hv(5.5, true, true) == 3
    @test hv(-Inf, true, true) == 0
    @test_throws InexactError hv(NaN, true, true)
    @test_throws InexactError hv(Inf, true, true)
    @test_throws InexactError hv(-1.0e30, true, false)
    @test hv(Inf, true, false) == 0

    # The raised InexactError is catchable and carries the upstream type.
    caught = try
        hc(NaN, true)
        nothing
    catch e
        e
    end
    @test caught isa InexactError
end

true
