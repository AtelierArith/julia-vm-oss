using Test

unitrange_collect_runtime_4266(x::Any) = collect(x)

@testset "UnitRange collect Base dispatch bridge (Issue #4266)" begin
    direct = collect(1:4)
    @test direct == [1, 2, 3, 4]
    @test typeof(direct) == Vector{Int64}

    runtime = unitrange_collect_runtime_4266(2:5)
    @test runtime == [2, 3, 4, 5]
    @test typeof(runtime) == Vector{Int64}

    direct_stepped = collect(1:2:7)
    @test direct_stepped == [1, 3, 5, 7]
    @test typeof(direct_stepped) == Vector{Int64}

    direct_reverse = collect(5:-1:1)
    @test direct_reverse == [5, 4, 3, 2, 1]
    @test typeof(direct_reverse) == Vector{Int64}

    stepped = unitrange_collect_runtime_4266(1:2:7)
    @test stepped == [1, 3, 5, 7]
    @test typeof(stepped) == Vector{Int64}

    direct_float = collect(1.0:0.5:2.0)
    @test direct_float == [1.0, 1.5, 2.0]
    @test typeof(direct_float) == Vector{Float64}

    runtime_float = unitrange_collect_runtime_4266(1.0:0.5:2.0)
    @test runtime_float == [1.0, 1.5, 2.0]
    @test typeof(runtime_float) == Vector{Float64}
end

true
