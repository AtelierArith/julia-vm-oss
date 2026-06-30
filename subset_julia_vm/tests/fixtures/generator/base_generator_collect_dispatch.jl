using Test

double(x) = x * 2
to_float(x) = x + 0.5
runtime_collect(x) = collect(x)

@testset "Base.Generator collect dispatch (Issue #4068)" begin
    ints = collect(Base.Generator(double, [1, 2, 3]))
    @test typeof(ints) === Vector{Int64}
    @test eltype(ints) === Int64
    @test ints == [2, 4, 6]

    runtime_ints = runtime_collect(Base.Generator(double, [1, 2, 3]))
    @test typeof(runtime_ints) === Vector{Int64}
    @test eltype(runtime_ints) === Int64
    @test runtime_ints == [2, 4, 6]

    floats = collect(Base.Generator(to_float, [1, 2, 3]))
    @test typeof(floats) === Vector{Float64}
    @test eltype(floats) === Float64
    @test floats == [1.5, 2.5, 3.5]

    runtime_floats = runtime_collect(Base.Generator(to_float, [1, 2, 3]))
    @test typeof(runtime_floats) === Vector{Float64}
    @test eltype(runtime_floats) === Float64
    @test runtime_floats == [1.5, 2.5, 3.5]

    empty = collect(Base.Generator(double, Int64[]))
    @test typeof(empty) === Vector{Int64}
    @test eltype(empty) === Int64
    @test length(empty) == 0

    runtime_empty = runtime_collect(Base.Generator(double, Int64[]))
    @test typeof(runtime_empty) === Vector{Int64}
    @test eltype(runtime_empty) === Int64
    @test length(runtime_empty) == 0

    empty_floats = collect(Base.Generator(to_float, Int64[]))
    @test typeof(empty_floats) === Vector{Float64}
    @test eltype(empty_floats) === Float64
    @test length(empty_floats) == 0

    empty_from_float_input = collect(Base.Generator(double, Float64[]))
    @test typeof(empty_from_float_input) === Vector{Float64}
    @test eltype(empty_from_float_input) === Float64
    @test length(empty_from_float_input) == 0
end

true
