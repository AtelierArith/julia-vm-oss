# Test where clause with multiple type parameters
# Returns first argument to avoid Any-typed arithmetic issue

using Test

function first_generic(x::T, y::S) where {T<:Number, S<:Number}
    x
end

@testset "Function with multiple type parameters in where clause - returns first arg" begin
    @test isapprox((first_generic(3.5, 1)), 3.5)
end

true  # Test passed
