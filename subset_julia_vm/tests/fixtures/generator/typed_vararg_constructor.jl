using Test

@testset "Typed vararg Base.Generator constructor (Issue #4109)" begin
    values = collect(Base.Generator(Complex{Int64}, [1, 2, 3], [10, 20]))
    @test typeof(values) === Vector{Complex{Int64}}
    @test eltype(values) === Complex{Int64}
    @test length(values) == 2
    @test values[1] == Complex{Int64}(1, 10)
    @test values[2] == Complex{Int64}(2, 20)

    g = Base.Generator(Complex{Int64}, [1, 2, 3], [10, 20])
    first = iterate(g)
    @test first[1] == Complex{Int64}(1, 10)
    @test typeof(first[1]) === Complex{Int64}
    second = iterate(g, first[2])
    @test second[1] == Complex{Int64}(2, 20)
    @test typeof(second[1]) === Complex{Int64}
    @test iterate(g, second[2]) === nothing

    short_first = collect(Base.Generator(Complex{Int64}, [1], [10, 20, 30]))
    @test length(short_first) == 1
    @test short_first[1] == Complex{Int64}(1, 10)

    empty_first = collect(Base.Generator(Complex{Int64}, Int64[], [10, 20]))
    @test typeof(empty_first) === Vector{Complex{Int64}}
    @test eltype(empty_first) === Complex{Int64}
    @test length(empty_first) == 0

    empty_second = collect(Base.Generator(Complex{Int64}, [1, 2], Int64[]))
    @test typeof(empty_second) === Vector{Complex{Int64}}
    @test eltype(empty_second) === Complex{Int64}
    @test length(empty_second) == 0

    @test_throws MethodError collect(Base.Generator(Float64, [1, 2], [10, 20]))
    @test_throws MethodError Float64(1, 10)
    T = Float64
    @test_throws MethodError T(1, 10)

    bad_empty = collect(Base.Generator(Float64, Int64[], [10, 20]))
    @test typeof(bad_empty) === Vector{Union{}}
    @test eltype(bad_empty) === Union{}
    @test length(bad_empty) == 0
end

true
