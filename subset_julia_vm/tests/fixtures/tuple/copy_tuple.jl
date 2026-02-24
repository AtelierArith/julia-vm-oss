# Test copy(Tuple) returns the same tuple (identity for immutable types)

using Test

@testset "copy(Tuple) returns same tuple" begin
    t = (1, 2, 3)
    ct = copy(t)
    @test ct == (1, 2, 3)
    @test length(ct) == 3

    # copy of empty tuple
    et = ()
    cet = copy(et)
    @test cet == ()
    @test length(cet) == 0

    # copy of single-element tuple
    st = (42,)
    cst = copy(st)
    @test cst == (42,)
    @test length(cst) == 1
end

true
