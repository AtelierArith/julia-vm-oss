# Test Pure Julia complex functions called through wrapper functions
# This tests fix for issue #932 - function calls from within functions
# returning incorrect results for Complex types.

using Test

# Test conj through wrapper
function wrap_conj(x)
    return conj(x)
end

# Test imag through wrapper
function wrap_imag(x)
    return imag(x)
end

# Test real through wrapper
function wrap_real(x)
    return real(x)
end

@testset "Complex functions through wrappers" begin
    z = 1.0 + 2.0im

    # Direct calls should work
    @test real(z) == 1.0
    @test imag(z) == 2.0
    @test conj(z) == 1.0 - 2.0im

    # Wrapped calls should also work (issue #932)
    @test wrap_real(z) == 1.0
    @test wrap_imag(z) == 2.0
    @test wrap_conj(z) == 1.0 - 2.0im
end

true
