# Test Base.tuple_type_head / tuple_type_tail / tuple_type_cons (Issue #5119)
# Type-level decomposition and construction of Tuple types. These are Base
# internals (not exported), so they are referenced through the `Base.` prefix.

using Test

@testset "tuple_type_head/tail/cons: Tuple type-parameter manipulation (Issue #5119)" begin
    # tuple_type_head: first parameter of a Tuple type
    @test Base.tuple_type_head(Tuple{Int,String}) === Int
    @test Base.tuple_type_head(Tuple{Int,String,Float64}) === Int
    @test Base.tuple_type_head(Tuple{Float64}) === Float64

    # tuple_type_tail: all parameters but the first, as a Tuple type
    @test Base.tuple_type_tail(Tuple{Int,String}) === Tuple{String}
    @test Base.tuple_type_tail(Tuple{Int,Float64}) === Tuple{Float64}
    @test Base.tuple_type_tail(Tuple{Int,String,Float64}) === Tuple{String,Float64}
    @test Base.tuple_type_tail(Tuple{Int}) === Tuple{}

    # tuple_type_cons: prepend a type to a Tuple type
    @test Base.tuple_type_cons(Int, Tuple{String}) === Tuple{Int,String}
    @test Base.tuple_type_cons(Float64, Tuple{Int,String}) === Tuple{Float64,Int,String}
    @test Base.tuple_type_cons(Int, Tuple{}) === Tuple{Int}

    # cons edge case: a Union{} tail yields Union{}
    @test Base.tuple_type_cons(Int, Union{}) === Union{}

    # round-trip: cons of head and tail reconstructs the original Tuple type
    T = Tuple{Int,String,Float64}
    @test Base.tuple_type_cons(Base.tuple_type_head(T), Base.tuple_type_tail(T)) === T
end

true  # Test passed
