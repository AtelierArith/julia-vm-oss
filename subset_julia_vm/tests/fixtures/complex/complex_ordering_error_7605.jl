# Regression for Issue #7605: Complex/Real ordering comparisons must error.
#
# `<`, `<=`, `>`, `>=` between a Complex and a Real (either operand order) are a
# MethodError upstream — Complex numbers are not orderable. In this VM the generic
# `<(x::Real, y::Real)` (promotion.jl) loosely matched a Complex operand under
# specialization and silently compared *real parts*, returning a Bool
# (e.g. `complex(1.0, 2.0) < 3` => true). Mirroring the `==(Complex, Real)`
# exclusion pattern (Issue #5966), explicit parametric `Complex{T} where {T<:Real}`
# ordering methods now reject the comparison with an error.
#
# Assertions use the abstract `Exception` supertype rather than a concrete type so
# the fixture stays parity-clean against upstream `julia` (CLAUDE.md design
# principle #4): both implementations *throw* here, even though the concrete type
# differs (sjulia raises an ErrorException("Complex numbers are not orderable");
# upstream raises a MethodError). The essential, cross-implementation behavior this
# locks is "ordering a Complex against a Real errors instead of returning a Bool".
# The sjulia-specific error message is pinned separately by the Rust integration
# test `test_complex_ordering_error`.
#
# Complex × Complex already raised a MethodError (neither operand matches `Real`),
# so it is left untouched and asserted here only to lock that correct behavior.

using Test

@testset "Complex/Real ordering errors, Complex on the left (Issue #7605)" begin
    z = complex(1.0, 2.0)
    @test_throws Exception z < 3
    @test_throws Exception z < 3.0
    @test_throws Exception z <= 3
    @test_throws Exception z <= 3.0
    @test_throws Exception z > 3
    @test_throws Exception z >= 3
end

@testset "Complex/Real ordering errors, Real on the left (Issue #7605)" begin
    z = complex(1.0, 2.0)
    @test_throws Exception 3 < z
    @test_throws Exception 3.0 < z
    @test_throws Exception 3 <= z
    @test_throws Exception 1.0 < z
    @test_throws Exception 3 > z
    @test_throws Exception 3 >= z
end

@testset "Complex{Int64}/Real ordering also errors (Issue #7605)" begin
    w = complex(1, 2)   # Complex{Int64}
    @test_throws Exception w < 3
    @test_throws Exception 3 < w
    @test_throws Exception w <= 3.0
    @test_throws Exception 3.0 >= w
end

@testset "Complex/Complex ordering stays an error too (Issue #7605)" begin
    # Neither operand matches `Real`, so this path is already correct upstream-style
    # (a MethodError); asserted to ensure the fix does not silently make it return.
    z = complex(1.0, 2.0)
    @test_throws Exception z < complex(2.0, 1.0)
    @test_throws Exception z <= complex(2.0, 1.0)
end

true
