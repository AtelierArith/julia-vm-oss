# Regression for Issue #5966: mixed Complex/Real `==`/`!=` must terminate.
#
# Before the fix, `Real == Complex` (e.g. `1 == Complex{Int64}(1, 0)`) matched
# only the promote-based `==(x::Number, y::Number)` fallback. When
# `promote(Real, Complex)` failed to widen the Real operand, that fallback
# re-dispatched `==(::Number, ::Number)` on the same `(Real, Complex)` pair
# forever — an unbounded VM call stack that OOM'd the full test suite
# non-deterministically. Adding the upstream `==(z::Complex, x::Real)` /
# `==(x::Real, z::Complex)` methods (and their `!=` counterparts) terminates.

using Test

@testset "mixed Complex/Real equality (Issue #5966)" begin
    # Complex with zero imaginary part equals the matching Real.
    @test Complex{Int64}(1, 0) == 1
    @test 1 == Complex{Int64}(1, 0)
    @test Complex(2.0, 0.0) == 2
    @test 2 == Complex(2.0, 0.0)
    @test Complex{Int64}(3, 0) == 3.0

    # A nonzero imaginary part means it is not equal to any Real.
    @test !(Complex{Int64}(1, 2) == 1)
    @test !(im == 1)
    @test !(1 == im)

    # `!=` mirrors `==`.
    @test Complex(2.0, 1.0) != 2
    @test 2 != Complex(2.0, 1.0)
    @test !(Complex{Int64}(5, 0) != 5)

    # The mixed-array promotion path that triggered the hang still works.
    a = [1, 2, im]
    @test a[1] == Complex{Int64}(1, 0)
    @test a[1] == 1
end

true
