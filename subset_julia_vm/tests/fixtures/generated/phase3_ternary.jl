# Phase 3 ternary operator in quotes

using Test

function abs_gen(x)
    if @generated
        :(x > 0 ? x : -x)
    else
        x > 0 ? x : -x
    end
end

@testset "Phase 3 ternary" begin
    @test abs_gen(5) == 5
    @test abs_gen(-3) == 3
    @test abs_gen(0) == 0
end

true
