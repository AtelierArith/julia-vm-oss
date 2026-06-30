using Test

function side_effect_value!(seen, value)
    push!(seen, value)
    return value
end

@testset "mixed String equality fallback (#4642)" begin
    @test (1 == "a") === false
    @test ("a" == 1) === false
    @test (1 != "a") === true
    @test ("a" != 1) === true
    @test ("a" == :a) === false
    @test ("a" == 'a') === false

    seen = Any[]
    @test (side_effect_value!(seen, "a") == side_effect_value!(seen, 1)) === false
    @test seen == Any["a", 1]

    values = Any[1, "a", 'a', :a]
    @test (values[1] == values[2]) === false
    @test (values[2] == values[1]) === false
    @test (values[2] != values[1]) === true
    @test (values[2] == values[3]) === false
    @test (values[2] == values[4]) === false
    @test isequal(values[1], values[2]) === false
    @test isequal(values[2], values[1]) === false
end

true
