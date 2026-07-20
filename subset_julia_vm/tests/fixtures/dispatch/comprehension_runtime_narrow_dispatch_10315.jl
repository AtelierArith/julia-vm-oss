# Issue #10315: an untyped comprehension whose body is statically unresolved
# can runtime-typejoin to a concrete Vector{T}. Dispatch must use that runtime
# element type rather than treating the compiler's Any placeholder as proof of
# Vector{Any}. Verified against upstream Julia 1.12.

using Test

narrow_int(x) = x < 0 ? "negative" : x
mixed_value(x) = x == 1 ? x : string(x)

which_vector(x::Vector{Any}) = :any
which_vector(x::Vector{Int64}) = :int

@testset "comprehension runtime element dispatch (Issue #10315)" begin
    # Indexed range path, both assigned and passed inline.
    values = [narrow_int(i) for i in 1:3]
    @test typeof(values) == Vector{Int64}
    @test which_vector(values) == :int
    @test which_vector([narrow_int(i) for i in 1:3]) == :int

    # Set takes the iteration-protocol path rather than indexed collection
    # lowering. Element order is irrelevant to these assertions.
    set_values = [narrow_int(i) for i in Set([1, 2, 3])]
    @test typeof(set_values) == Vector{Int64}
    @test which_vector(set_values) == :int

    # Tuple destructuring has its own runtime-typejoin lowering path.
    pair_values = [narrow_int(i) for (i, _) in [(1, 10), (2, 20), (3, 30)]]
    @test typeof(pair_values) == Vector{Int64}
    @test which_vector(pair_values) == :int

    # Controls: genuinely heterogeneous and explicitly typed comprehensions
    # are concrete Vector{Any} values and must keep selecting that overload.
    heterogeneous = [mixed_value(i) for i in 1:3]
    @test typeof(heterogeneous) == Vector{Any}
    @test which_vector(heterogeneous) == :any

    explicitly_any = Any[narrow_int(i) for i in 1:3]
    @test typeof(explicitly_any) == Vector{Any}
    @test which_vector(explicitly_any) == :any
end

true
