using Test

@testset "Dict pair destructuring comprehension (#4632)" begin
    d = Dict(Int8(1) => Int16(2))

    keys = [k for (k, v) in d]
    @test typeof(keys) === Vector{Int8}
    @test eltype(keys) === Int8
    @test length(keys) == 1
    @test keys[1] === Int8(1)

    values = [v for (k, v) in d]
    @test typeof(values) === Vector{Int16}
    @test eltype(values) === Int16
    @test length(values) == 1
    @test values[1] === Int16(2)
end

true
