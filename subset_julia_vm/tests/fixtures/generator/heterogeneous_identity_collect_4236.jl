using Test

function generator_heterogeneous_identity_call_4236(f, itr)
    return collect(Base.Generator(f, itr))
end

@testset "heterogeneous identity generator collect (Issue #4236)" begin
    values = collect(Base.Generator(identity, (1, 2.0)))
    @test typeof(values) === Vector{Real}
    @test eltype(values) === Real
    @test values[1] == 1
    @test typeof(values[1]) === Int64
    @test values[2] == 2.0
    @test typeof(values[2]) === Float64

    dynamic_values = generator_heterogeneous_identity_call_4236(identity, (1, 2.0))
    @test typeof(dynamic_values) === Vector{Real}
    @test eltype(dynamic_values) === Real
    @test dynamic_values[1] == 1
    @test typeof(dynamic_values[1]) === Int64
    @test dynamic_values[2] == 2.0
    @test typeof(dynamic_values[2]) === Float64

    memory_values = Base.collect_similar(
        Memory{Float64}(undef, 0),
        Base.Generator(identity, (1, 2.0)),
    )
    @test typeof(memory_values) === Memory{Real}
    @test eltype(memory_values) === Real
    @test memory_values[1] == 1
    @test typeof(memory_values[1]) === Int64
    @test memory_values[2] == 2.0
    @test typeof(memory_values[2]) === Float64
end

true
