# Test supertype operator >:

using Test

@testset "supertype operator >:" begin

    # A >: B means B <: A (A is supertype of B)
    @assert Number >: Int64
    @assert Real >: Float64
    @assert Integer >: Int64
    @assert Any >: Number

    # Reflexive: A >: A
    @assert Int64 >: Int64
    @assert Number >: Number

    # False cases
    @assert !(Int64 >: Float64)
    @assert !(Signed >: Float64)

    @test (true)
end

true  # Test passed
