using Test

@testset "generator collect routes public materialization to Array wrapper (#6649)" begin
    eager = collect(x + 1 for x in 1:3)
    @test typeof(eager) === Vector{Int64}
    @test typeof(eager.ref) == MemoryRef{Int64}
    @test eager == [2, 3, 4]

    runtime_callable = collect(Base.Generator(x -> x + 1, 1:3))
    @test typeof(runtime_callable) === Vector{Int64}
    @test typeof(runtime_callable.ref) == MemoryRef{Int64}
    @test runtime_callable == [2, 3, 4]

    f(x) = x + 1
    function_callable = collect(Base.Generator(f, 1:3))
    @test typeof(function_callable) === Vector{Int64}
    @test typeof(function_callable.ref) == MemoryRef{Int64}
    @test function_callable == [2, 3, 4]

    filtered = collect(x + 10 for x in 1:5 if isodd(x))
    @test typeof(filtered) === Vector{Int64}
    @test typeof(filtered.ref) == MemoryRef{Int64}
    @test filtered == [11, 13, 15]

    tuple_splat = collect(x + y for (x, y) in zip(1:3, 4:6))
    @test typeof(tuple_splat) === Vector{Int64}
    @test typeof(tuple_splat.ref) == MemoryRef{Int64}
    @test tuple_splat == [5, 7, 9]
end

true
