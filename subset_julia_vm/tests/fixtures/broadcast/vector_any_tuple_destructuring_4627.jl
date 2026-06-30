using Test

@testset "Vector{Any} tuple elements destructure in for loops (#4627)" begin
    cases = Any[("f32add", Float32[1, 2] .+ Float32[3, 4])]

    @test typeof(cases) === Vector{Any}
    @test length(cases) == 1
    @test typeof(cases[1]) === Tuple{String, Vector{Float32}}

    seen_name = ""
    seen_eltype = Any
    seen_first = Float32(0)
    for (name, values) in cases
        seen_name = name
        seen_eltype = eltype(values)
        seen_first = values[1]
    end

    @test seen_name == "f32add"
    @test seen_eltype === Float32
    @test seen_first === Float32(4)
end

true
