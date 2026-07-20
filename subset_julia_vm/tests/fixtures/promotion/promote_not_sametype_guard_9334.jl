using Test

# Issue #9334: `promote(x, y)` must raise the upstream not_sametype /
# sametype_error when promotion cannot change EITHER argument's type. Without
# the guard sjulia silently returned the unchanged pair, so the promote-fallback
# operators (`==`/`+`/`<` defined as `op(promote(x, y)...)`) re-dispatched on the
# identical pair forever -> StackOverflowError. Upstream raises immediately with
# `ErrorException("promotion of types A and B failed to change any arguments")`.

struct S2 <: Real
    v::Float64
end

@testset "promote raises not_sametype error for unpromotable mixed pair (Issue #9334)" begin
    # No promote_rule / convert defined for (S2, Int64): promotion cannot change
    # either type, so upstream errors instead of looping.
    @test_throws ErrorException promote(S2(1.0), 1)
    @test_throws ErrorException promote(1, S2(1.0))

    err = try
        promote(S2(1.0), 1)
        nothing
    catch e
        e
    end
    @test err isa ErrorException
    @test occursin("failed to change any arguments", err.msg)
    @test occursin("S2", err.msg)
    @test occursin("Int64", err.msg)
end

@testset "promote-fallback arithmetic surfaces the error, not StackOverflow (Issue #9334)" begin
    # `+(x::Number, y::Number) = +(promote(x, y)...)` used to recurse forever.
    @test_throws ErrorException S2(1.0) + 1
    @test_throws ErrorException S2(1.0) * 1
    @test_throws ErrorException 1 - S2(1.0)
end

@testset "same-type promote still returns the pair unchanged (Issue #9334)" begin
    # The guard must not fire for an already-same-type pair (upstream reaches
    # this through the diagonal `promote(x::T, y::T)` method).
    p = promote(S2(1.0), S2(2.0))
    @test typeof(p[1]) === S2
    @test typeof(p[2]) === S2
    @test p[1].v == 1.0
    @test p[2].v == 2.0
end

@testset "same-type numeric equality anchor prevents promote fallback recursion (Issue #9334)" begin
    # Upstream has a same-type Number equality anchor below the promote
    # fallback: equality falls back to `===` instead of recursively promoting
    # the unchanged pair.
    same = S2(1.0)
    @test same == same
    @test !(S2(1.0) == S2(2.0))
end

@testset "same-type fallback operators fail fast instead of recursing (Issue #9334)" begin
    @test_throws ErrorException S2(1.0) + S2(2.0)
    @test_throws ErrorException S2(1.0) * S2(2.0)
    @test_throws ErrorException S2(1.0) < S2(2.0)
    @test_throws ErrorException S2(1.0) <= S2(2.0)
    @test_throws ErrorException S2(1.0) > S2(2.0)
    @test_throws ErrorException S2(1.0) >= S2(2.0)

    err = try
        S2(1.0) < S2(2.0)
        nothing
    catch e
        e
    end
    @test err isa ErrorException
    @test occursin("< not defined for S2", err.msg)
end

@testset "guard does not disturb ordinary numeric promotion (Issue #9334)" begin
    @test promote(1, 2.0) === (1.0, 2.0)
    @test promote(Int8(3), 2.0f0) === (3.0f0, 2.0f0)
    @test (1 + 2.0) === 3.0
    @test (2 // 3 + 1) == 5 // 3
    @test (1 == 1.0)
end

true
