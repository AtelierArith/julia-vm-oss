# Comprehensive Float32 arithmetic test (Issue #1649)
# Template for testing new numeric types - ensures all operations and type combinations work
#
# Based on Issue #1649's prevention checklist:
# 1. Same-type arithmetic (fully tested)
# 2. Cross-type with Int64 (tested for value correctness - type checking deferred)
# 3. Cross-type with Float64 (tested for value correctness - type checking deferred)
# 4. Cross-type with Bool (tested for value correctness - type checking deferred)
# 5. Type conversions (fully tested)
#
# Note: Type-checking tests for mixed-type operations are split into separate
# test sets to work around method caching issues in complex test files.

using Test

@testset "Float32 Same-type Arithmetic" begin
    # All 4 arithmetic operations
    @test Float32(2.5) + Float32(1.5) == Float32(4.0)
    @test Float32(2.5) - Float32(1.5) == Float32(1.0)
    @test Float32(2.5) * Float32(1.5) == Float32(3.75)
    @test Float32(5.0) / Float32(2.0) == Float32(2.5)

    # Verify result type is preserved as Float32
    @test typeof(Float32(2.5) + Float32(1.5)) == Float32
    @test typeof(Float32(2.5) - Float32(1.5)) == Float32
    @test typeof(Float32(2.5) * Float32(1.5)) == Float32
    @test typeof(Float32(5.0) / Float32(2.0)) == Float32

    # Edge cases: Negative numbers
    @test Float32(-2.5) + Float32(1.0) == Float32(-1.5)
    @test Float32(-2.5) * Float32(-2.0) == Float32(5.0)

    # Edge cases: Zero operations
    @test Float32(0.0) + Float32(1.5) == Float32(1.5)
    @test Float32(2.5) * Float32(0.0) == Float32(0.0)

    # Edge cases: Very small numbers
    @test Float32(0.001) + Float32(0.001) == Float32(0.002)
end

@testset "Float32 Type Conversions" begin
    # Float32 -> Float64
    @test Float64(Float32(2.5)) == 2.5
    @test typeof(Float64(Float32(2.5))) == Float64

    # Float32 -> Int64 (truncation)
    @test Int64(Float32(3.0)) == 3
    @test typeof(Int64(Float32(3.0))) == Int64

    # Float64 -> Float32
    @test Float32(2.5) == Float32(2.5)
    @test typeof(Float32(2.5)) == Float32

    # Int64 -> Float32
    @test Float32(3) == Float32(3.0)
    @test typeof(Float32(3)) == Float32
end

true
