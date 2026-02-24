# Phase 3 multiple statements in quotes

using Test

function multi_gen(x, y)
    if @generated
        a = :(x + 1)
        b = :(y * 2)
        :(a + b)
    else
        a = x + 1
        b = y * 2
        a + b
    end
end

@testset "Phase 3 multiple statements" begin
    @test multi_gen(5, 3) == 12  # (5+1) + (3*2) = 6 + 6 = 12
    @test multi_gen(0, 0) == 1   # (0+1) + (0*2) = 1 + 0 = 1
end

true
