using Test

@testset "Memory collect traits" begin
    m = Memory{Int64}(undef, 2)
    m[1] = 1
    m[2] = 2

    @test typeof(Base.IteratorSize(m)) === Base.HasShape{1}
    @test typeof(Base.IteratorEltype(m)) === typeof(Base.HasEltype())

    values = collect(m)
    @test typeof(values) === Vector{Int64}
    @test values == [1, 2]

    similar_values = Base.collect_similar(Float64[], m)
    @test typeof(similar_values) === Vector{Int64}
    @test similar_values == [1, 2]

    converted = collect(Float64, m)
    @test typeof(converted) === Vector{Float64}
    @test converted == [1.0, 2.0]
end

true
