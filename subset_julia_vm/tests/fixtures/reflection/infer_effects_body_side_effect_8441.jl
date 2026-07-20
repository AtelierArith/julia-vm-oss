using Test

function effectful_body_8441(x)
    println(x)
    return x + 1
end

@testset "body-derived effects detect side-effecting user methods (Issue #8441)" begin
    ef = Base.infer_effects(effectful_body_8441, Tuple{Int64})
    @test ef.effect_free != 0x00
    @test ef.nothrow === false
end

true
