using Test

generator_public_collect_inc_4265(x) = x + 1
generator_public_collect_float_4265(x) = x + 0.5
generator_public_collect_pair_4265(x, y) = x + y
generator_public_collect_double_4265(x) = x * 2
generator_public_collect_even_4265(x) = x % 2 == 0

generator_public_collect_runtime_4265(x::Any) = collect(x)

@testset "public collect generator generic path boundary (Issue #4265)" begin
    direct = collect(Base.Generator(generator_public_collect_inc_4265, [1, 2, 3]))
    @test direct == [2, 3, 4]
    @test typeof(direct) === Vector{Int64}

    runtime = generator_public_collect_runtime_4265(
        Base.Generator(generator_public_collect_inc_4265, [4, 5, 6]),
    )
    @test runtime == [5, 6, 7]
    @test typeof(runtime) === Vector{Int64}

    matrix_direct = collect(Base.Generator(generator_public_collect_inc_4265, [1 2; 3 4]))
    @test matrix_direct == [2 3; 4 5]
    @test typeof(matrix_direct) === Matrix{Int64}
    @test size(matrix_direct) == (2, 2)

    matrix_runtime = generator_public_collect_runtime_4265(
        Base.Generator(generator_public_collect_float_4265, [1 2; 3 4]),
    )
    @test matrix_runtime == [1.5 2.5; 3.5 4.5]
    @test typeof(matrix_runtime) === Matrix{Float64}
    @test size(matrix_runtime) == (2, 2)

    pair = generator_public_collect_runtime_4265(
        Base.Generator(generator_public_collect_pair_4265, [1, 2, 3], [10, 20]),
    )
    @test pair == [11, 22]
    @test typeof(pair) === Vector{Int64}

    filtered = generator_public_collect_runtime_4265(
        (generator_public_collect_double_4265(x) for x in 1:6 if generator_public_collect_even_4265(x)),
    )
    @test filtered == [4, 8, 12]
    @test typeof(filtered) === Vector{Int64}

    empty = generator_public_collect_runtime_4265(
        Base.Generator(generator_public_collect_float_4265, Int64[]),
    )
    @test typeof(empty) === Vector{Float64}
    @test eltype(empty) === Float64
    @test length(empty) == 0
end

true
