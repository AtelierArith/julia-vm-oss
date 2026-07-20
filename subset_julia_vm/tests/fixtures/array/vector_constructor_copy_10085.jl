# Vector(::Vector) constructor copies and works as a callable (Issue #10085)

using Test

@testset "Vector(::Vector) copies (Issue #10085)" begin
    src = [1, 2, 3]
    direct = Vector(src)
    typed = Vector{Int64}(src)

    @test direct == src
    @test typed == src
    @test typeof(direct) === Vector{Int64}
    @test typeof(typed) === Vector{Int64}
    @test !(direct === src)
    @test !(typed === src)

    direct[1] = 10
    typed[2] = 20
    @test src == [1, 2, 3]
    @test direct == [10, 2, 3]
    @test typed == [1, 20, 3]
end

@testset "map(Vector, ::Vector{Vector}) copies each element (Issue #10085)" begin
    xs = [[1], [2]]
    ys = map(Vector, xs)

    @test ys == [[1], [2]]
    # Outer eltype precision for this HOF path is tracked separately in #10187.
    @test typeof(ys[1]) === Vector{Int64}
    @test !(ys[1] === xs[1])
    @test !(ys[2] === xs[2])

    ys[1][1] = 99
    @test xs == [[1], [2]]
    @test ys == [[99], [2]]
end

true
