# isbitstype(T) / isbits(x) with correct nested-struct recursion (Issue #5104).
#
# Upstream rule: a type is isbits iff it is a concrete, immutable `DataType`
# whose every field type is itself isbits (primitive numeric types, `Bool`,
# `Char`, `Nothing`, `Missing`, and immutable structs of isbits fields, applied
# recursively). Mutable structs, abstract types, `UnionAll`s, and types with a
# non-isbits field (`String`, arrays, mutable references) are NOT isbits.
# Verified against upstream Julia 1.12: every assertion below matches `julia`.

using Test

struct Bits5104
    a::Int
    b::Float64
end

struct Str5104
    s::String
end

struct Arr5104
    v::Vector{Int}
end

mutable struct Mut5104
    a::Int
end

struct NestedBits5104
    p::Bits5104
    q::Bool
end

struct NestedStr5104
    p::Bits5104
    s::Str5104
end

struct DeepBits5104
    n::NestedBits5104
    c::Char
end

struct WrapsMut5104
    m::Mut5104
end

abstract type Abs5104 end

struct Param5104{T}
    x::T
    y::T
end

struct Pair5104{A,B}
    x::A
    y::B
end

struct Empty5104 end

@testset "isbitstype nested-struct recursion (Issue #5104)" begin
    # Primitives / builtin leaves.
    @test isbitstype(Int)
    @test isbitstype(Float64)
    @test isbitstype(Bool)
    @test isbitstype(Char)
    @test isbitstype(Int8)
    @test isbitstype(UInt64)
    @test isbitstype(Nothing)
    @test isbitstype(Missing)
    @test !isbitstype(String)
    @test !isbitstype(Symbol)
    @test !isbitstype(BigInt)
    @test !isbitstype(BigFloat)

    # Plain immutable bits struct vs. non-bits field structs.
    @test isbitstype(Bits5104)
    @test !isbitstype(Str5104)
    @test !isbitstype(Arr5104)
    @test !isbitstype(Mut5104)

    # Nested immutable bits structs recurse to true; a nested String field
    # makes the outer struct non-bits; a struct wrapping a mutable struct is
    # likewise non-bits.
    @test isbitstype(NestedBits5104)
    @test isbitstype(DeepBits5104)
    @test !isbitstype(NestedStr5104)
    @test !isbitstype(WrapsMut5104)

    # Abstract types and bare (un-instantiated) parametric types are not bits.
    @test !isbitstype(Abs5104)
    @test !isbitstype(Param5104)

    # Parametric concrete instantiations are classified by their substituted
    # field types.
    @test isbitstype(Param5104{Int})
    @test isbitstype(Param5104{Float64})
    @test isbitstype(Param5104{Bool})
    @test !isbitstype(Param5104{String})
    @test isbitstype(Pair5104{Int,Float64})
    @test !isbitstype(Pair5104{Int,String})
    @test isbitstype(Pair5104{Bits5104,Bool})

    # Empty immutable struct is bits (vacuously).
    @test isbitstype(Empty5104)

    # Builtin parametric wrappers / collections.
    @test isbitstype(Complex{Float64})
    @test isbitstype(Complex{Int})
    @test !isbitstype(Complex)
    @test isbitstype(Rational{Int})
    @test !isbitstype(Vector{Int})

    # Tuple types recurse over their element types; the empty tuple is bits.
    @test isbitstype(Tuple{})
    @test isbitstype(Tuple{Int})
    @test isbitstype(Tuple{Int,Float64})
    @test isbitstype(NTuple{3,Int})
    @test isbitstype(Tuple{Bits5104,NestedBits5104})
    @test !isbitstype(Tuple{Int,String})
    @test !isbitstype(Tuple{Bits5104,Str5104})

    # isbits(x) === isbitstype(typeof(x)) for instances, including nested
    # structs, parametric instances, and Tuple values.
    @test isbits(1)
    @test isbits(3.14)
    @test isbits(true)
    @test isbits('a')
    @test isbits(nothing)
    @test isbits(missing)
    @test !isbits("hello")
    @test !isbits([1, 2, 3])
    @test isbits(Bits5104(1, 2.0))
    @test !isbits(Str5104("x"))
    @test isbits(NestedBits5104(Bits5104(1, 2.0), true))
    @test isbits(DeepBits5104(NestedBits5104(Bits5104(1, 2.0), true), 'z'))
    @test isbits(Param5104{Int}(1, 2))
    @test isbits(Param5104{Float64}(1.0, 2.0))
    @test isbits((1, 2.0, 'c'))
    @test !isbits((1, "s"))
end

true
