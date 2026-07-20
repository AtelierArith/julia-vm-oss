using Test

struct ImmutableFieldAssign10511
    a::Int
end

@testset "immutable field assignment is catchable (Issue #10511)" begin
    m = ImmutableFieldAssign10511(1)
    caught = false
    try
        m.a = 2
    catch e
        caught = e isa ErrorException
    end
    @test caught
    @test m.a == 1
end

true
