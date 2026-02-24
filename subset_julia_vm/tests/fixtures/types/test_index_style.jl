# Test IndexStyle abstract type and subtypes (IndexLinear, IndexCartesian)

using Test

@testset "IndexStyle types" begin
    # Test IndexStyle is an abstract type
    @test isabstracttype(IndexStyle)

    # Test IndexLinear and IndexCartesian exist
    @test isa(IndexLinear(), IndexLinear)
    @test isa(IndexCartesian(), IndexCartesian)

    # Test subtype relationships
    @test IndexLinear <: IndexStyle
    @test IndexCartesian <: IndexStyle

    # Test they are concrete types (not abstract)
    @test isconcretetype(IndexLinear)
    @test isconcretetype(IndexCartesian)
end

true  # Test passed
