using Test

# Issue #4851: Parametric default constructor inference must bind type
# parameters embedded inside nested field type expressions like Tuple{T,T}
# or Vector{T}, not just bare `field::T` fields.

struct NestedParamProbe4851{T}
    a::Tuple{T,T}
    b::String
end

struct VecParamProbe4851{T}
    a::Vector{T}
end

struct PairParamProbe4851{S,T}
    a::Tuple{S,T}
end

mk_nested_4851(flag) = NestedParamProbe4851((flag ? 1 : 2, 3), "y")
getb_nested_4851(flag) = getfield(mk_nested_4851(flag), :b)

@testset "Nested parametric constructor inference (Issue #4851)" begin
    v = mk_nested_4851(true)
    @test v isa NestedParamProbe4851
    @test v isa NestedParamProbe4851{Int64}
    @test typeof(v) == NestedParamProbe4851{Int64}
    @test v.a == (1, 3)
    @test typeof(v.a) == Tuple{Int64, Int64}
    @test v.b == "y"
    @test getb_nested_4851(true) == "y"

    # Vector{T} embedded field
    vp = VecParamProbe4851([1, 2, 3])
    @test vp isa VecParamProbe4851{Int64}
    @test typeof(vp) == VecParamProbe4851{Int64}
    @test vp.a == [1, 2, 3]

    # Multiple distinct type parameters inside one tuple field
    pp = PairParamProbe4851((1, "x"))
    @test pp isa PairParamProbe4851{Int64, String}
    @test typeof(pp) == PairParamProbe4851{Int64, String}
    @test pp.a == (1, "x")
end

true
