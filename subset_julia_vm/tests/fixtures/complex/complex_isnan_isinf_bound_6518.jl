# Regression for Issue #6518: `isnan`/`isinf`/`transpose` on `Complex{T} where
# T<:Real` must keep their `T<:Real` bound across the prelude-program /
# method-table caches.
#
# The `TypeParam.bound` legacy mirror of `upper_bound` is `#[serde(skip)]`, so a
# plain derive left it `None` after the prelude `Program` was bincode-round-
# tripped — silently dropping the `where T<:Real` constraint and making the
# caches non-transparent (the serde round-trip test diverged `bound: None` vs
# `Some("Real")`). The fix reconstructs `bound` from `upper_bound` on
# deserialization. This fixture pins the user-visible behavior: these methods
# must dispatch and evaluate exactly as upstream Julia for both Float and Int
# Complex element types.

using Test

@testset "Complex isnan/isinf/transpose bound (Issue #6518)" begin
    # isnan on Complex{Float64}
    @test isnan(1.0 + 2.0im) == false
    @test isnan(Complex(NaN, 0.0)) == true
    @test isnan(Complex(0.0, NaN)) == true

    # isinf on Complex{Float64}
    @test isinf(Complex(Inf, 0.0)) == true
    @test isinf(Complex(0.0, -Inf)) == true
    @test isinf(1.0 + 2.0im) == false

    # isnan/isinf on Complex{Int64} (never NaN/Inf)
    @test isnan(1 + 2im) == false
    @test isinf(1 + 2im) == false

    # transpose of a scalar Complex returns the value unchanged.
    @test transpose(1.0 + 2.0im) == 1.0 + 2.0im
    @test transpose(3 + 4im) == 3 + 4im
end

true
