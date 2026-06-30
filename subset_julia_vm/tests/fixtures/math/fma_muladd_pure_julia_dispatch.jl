# Test fma/muladd Pure Julia dispatch (Issue #3732)
#
# After the migration, public `fma` and `muladd` are routed through the
# Pure Julia method table (base/math.jl) rather than a Rust builtin.
# This fixture exercises:
#   - direct calls
#   - calls forwarded through a user-defined wrapper
#   - calls reached via a function reference (function-variable path)
#   - mixed integer/float argument types
# All cases must match official Julia.

using Test

# User-defined wrapper that goes through method dispatch on `fma`/`muladd`
fma_via_wrapper(x, y, z) = fma(x, y, z)
muladd_via_wrapper(x, y, z) = muladd(x, y, z)

# Function-variable / function-reference path: passing the function as a
# first-class value. With Pure Julia dispatch this must resolve to the
# Pure Julia `fma`/`muladd` method.
apply3(f, x, y, z) = f(x, y, z)

@testset "Pure Julia dispatch for fma / muladd (Issue #3732)" begin
    # Float64 direct calls (use IEEE fused semantics via internal _fma)
    @test (fma(2.0, 3.0, 4.0)) == 10.0
    @test (muladd(2.0, 3.0, 4.0)) == 10.0

    # Integer arguments — Pure Julia generic method computes x*y + z
    @test (fma(2, 3, 4)) == 10
    @test (muladd(2, 3, 4)) == 10

    # Mixed Float/Int — promotion still produces the right answer
    @test (fma(1.5, 2.0, 0.5)) == 3.5
    @test (muladd(1.5, 2.0, 0.5)) == 3.5

    # Negative values
    @test (fma(-1.0, 2.0, 3.0)) == 1.0
    @test (muladd(-1.0, 2.0, 3.0)) == 1.0

    # Zero
    @test (fma(0.0, 10.0, 5.0)) == 5.0
    @test (muladd(0.0, 10.0, 5.0)) == 5.0

    # User-defined wrapper goes through Pure Julia dispatch
    @test (fma_via_wrapper(2.0, 3.0, 4.0)) == 10.0
    @test (muladd_via_wrapper(2.0, 3.0, 4.0)) == 10.0

    # Function-variable path: passing fma/muladd as a first-class value
    # exercises the BuiltinId::from_name() splat/PushFunction path that
    # used to shadow Pure Julia. Must reach the Pure Julia method.
    @test (apply3(fma, 2.0, 3.0, 4.0)) == 10.0
    @test (apply3(muladd, 2.0, 3.0, 4.0)) == 10.0
end

true
