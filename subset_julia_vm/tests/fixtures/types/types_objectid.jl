# Test objectid function

using Test

@testset "objectid - get unique object identifier" begin

    # objectid returns UInt
    @assert typeof(objectid(1)) == UInt64
    @assert typeof(objectid(3.14)) == UInt64
    @assert typeof(objectid("hello")) == UInt64
    @assert typeof(objectid(nothing)) == UInt64
    @assert typeof(objectid(missing)) == UInt64

    # objectid works on various types
    @assert objectid([1, 2, 3]) isa UInt64
    @assert objectid((1, 2)) isa UInt64

    @test (true)
end

true  # Test passed
