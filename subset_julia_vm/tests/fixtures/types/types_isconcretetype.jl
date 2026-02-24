# Test isconcretetype function

using Test

@testset "isconcretetype - check if type is concrete" begin

    # Concrete types (can have instances)
    @assert isconcretetype(Int64)
    @assert isconcretetype(Float64)
    @assert isconcretetype(Bool)
    @assert isconcretetype(Char)
    @assert isconcretetype(String)
    @assert isconcretetype(Nothing)

    # Abstract types (cannot have instances directly)
    @assert !isconcretetype(Integer)
    @assert !isconcretetype(Real)
    @assert !isconcretetype(Number)
    @assert !isconcretetype(Any)
    @assert !isconcretetype(AbstractFloat)

    @test (true)
end

true  # Test passed
