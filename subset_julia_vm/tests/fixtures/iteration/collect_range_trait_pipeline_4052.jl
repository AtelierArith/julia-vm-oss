using Test

@testset "range collect trait pipeline (Issue #4052)" begin
    lin = LinRange(1.0, 3.0, 3)
    lin_size = Base.IteratorSize(lin)
    @test typeof(lin_size) === Base.HasShape{1}
    lin_values = collect(lin)
    @test typeof(lin_values) === Vector{Float64}
    @test eltype(lin_values) === Float64
    @test length(lin_values) == 3
    @test lin_values[1] == 1.0
    @test lin_values[2] == 2.0
    @test lin_values[3] == 3.0

    step = range(1.0, step=0.5, length=3)
    step_size = Base.IteratorSize(step)
    @test typeof(step_size) === Base.HasShape{1}
    step_values = collect(step)
    @test typeof(step_values) === Vector{Float64}
    @test eltype(step_values) === Float64
    @test length(step_values) == 3
    @test step_values[1] == 1.0
    @test step_values[2] == 1.5
    @test step_values[3] == 2.0

    one = Base.OneTo(3)
    one_size = Base.IteratorSize(one)
    @test typeof(one_size) === Base.HasShape{1}
    one_values = collect(one)
    @test typeof(one_values) === Vector{Int64}
    @test eltype(one_values) === Int64
    @test length(one_values) == 3
    @test one_values[1] == 1
    @test one_values[2] == 2
    @test one_values[3] == 3
end

true
