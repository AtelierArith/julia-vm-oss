using Test
using Iterators

@testset "collect(enumerate(...)) preserves tuple eltype (Issue #4145)" begin
    values = collect(enumerate([10, 20, 30]))
    @test typeof(values) === Vector{Tuple{Int64, Int64}}
    @test eltype(values) === Tuple{Int64, Int64}
    @test values == [(1, 10), (2, 20), (3, 30)]
end

@testset "collect(rest(...)) uses SizeUnknown grow path (Issue #4145)" begin
    arr = [10, 20, 30, 40]
    first_next = iterate(arr)
    r = rest(arr, first_next[2])
    values = collect(r)
    @test typeof(values) === Vector{Int64}
    @test eltype(values) === Int64
    @test values == [20, 30, 40]
end

@testset "collect(rest(enumerate(...))) keeps tuple values (Issue #4145)" begin
    e = enumerate([10, 20, 30])
    e_state = iterate(e)[2]
    values = collect(rest(e, e_state))
    @test values == [(2, 20), (3, 30)]
end

true
