# Test rest iterator semantics matching upstream Julia

using Test
using Base: IteratorEltype, IteratorSize, HasEltype, IsInfinite, SizeUnknown
using Base.Iterators

function materialize_rest_4142(itr)
    values = []
    next = iterate(itr)
    while next !== nothing
        push!(values, next[1])
        next = iterate(itr, next[2])
    end
    return values
end

@testset "rest(iter) returns the original iterator (Issue #4142)" begin
    arr = [10, 20, 30, 40]
    @test collect(rest(arr)) == arr
end

@testset "rest(iter, state) starts from the given state (Issue #4142)" begin
    arr = [10, 20, 30, 40]
    first_next = iterate(arr)
    @assert first_next !== nothing
    r = rest(arr, first_next[2])
    @test r.st == first_next[2]

    next = iterate(r)
    @test next !== nothing
    @test next[1] == 20

    next = iterate(r, next[2])
    @test next !== nothing
    @test next[1] == 30

    next = iterate(r, next[2])
    @test next !== nothing
    @test next[1] == 40

    next = iterate(r, next[2])
    @test next === nothing

    @test materialize_rest_4142(r) == [20, 30, 40]
    @test IteratorSize(r) isa SizeUnknown
    @test IteratorEltype(r) isa HasEltype
    @test eltype(r) == Int64
end

@testset "rest(array, explicit upstream state) starts at that index (Issue #4647)" begin
    arr = Int8[1, 2, 3]
    r = rest(arr, 2)
    values = collect(r)
    @test typeof(values) == Vector{Int8}
    @test eltype(values) == Int8
    @test values == Int8[2, 3]
end

@testset "rest unwraps nested Rest and preserves traits (Issue #4142)" begin
    arr = [10, 20, 30, 40]
    first_next = iterate(arr)
    r1 = rest(arr, first_next[2])
    second_next = iterate(r1)
    r2 = rest(r1, second_next[2])
    @test materialize_rest_4142(r2) == [30, 40]

    c = countfrom()
    c_state = iterate(c)[2]
    @test IteratorSize(rest(c, c_state)) isa IsInfinite

    e = enumerate([10, 20, 30])
    e_state = iterate(e)[2]
    @test materialize_rest_4142(rest(e, e_state)) == [(2, 20), (3, 30)]
end

true  # Test passed
