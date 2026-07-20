using Test
using Base.Iterators

struct CountNoIndex10442
    n::Int64
end

Base.length(c::CountNoIndex10442) = c.n

function Base.iterate(c::CountNoIndex10442)
    c.n == 0 && return nothing
    return (1, 2)
end

function Base.iterate(c::CountNoIndex10442, state)
    state > c.n && return nothing
    return (state, state + 1)
end

@testset "Comprehensions over iterate-only structs use iterate (Issue #10442)" begin
    q = partition([1, 2, 3, 4, 5], 2)
    partition_getindex_succeeded = true
    try
        q[1]
    catch
        partition_getindex_succeeded = false
    end
    @test !partition_getindex_succeeded

    chunks = [collect(chunk) for chunk in q]
    @test chunks == [[1, 2], [3, 4], [5]]

    inline_chunks = [collect(chunk) for chunk in partition([1, 2, 3, 4, 5], 2)]
    @test inline_chunks == chunks

    filtered = [collect(chunk) for chunk in q if length(chunk) == 2]
    @test filtered == [[1, 2], [3, 4]]

    doubled = [2x for x in CountNoIndex10442(4)]
    @test doubled == [2, 4, 6, 8]
    @test typeof(doubled) == Vector{Int64}
end

true
