# Test supertype function

using Test

@testset "supertype - get parent type in type hierarchy" begin

    # Test that supertype returns correct types by checking type names
    @assert string(supertype(Int64)) == "Signed"
    @assert string(supertype(UInt64)) == "Unsigned"
    @assert string(supertype(Float64)) == "AbstractFloat"
    @assert string(supertype(Bool)) == "Integer"
    @assert string(supertype(Char)) == "AbstractChar"

    # Abstract types
    @assert string(supertype(Signed)) == "Integer"
    @assert string(supertype(Integer)) == "Real"
    @assert string(supertype(Real)) == "Number"
    @assert string(supertype(Number)) == "Any"
    @assert string(supertype(AbstractFloat)) == "Number"

    # Any is its own supertype
    @assert string(supertype(Any)) == "Any"

    @test (true)
end

true  # Test passed
