# Issue #5061: `Tuple{Int, Vararg{T}}` and general-tuple intersection /
# subtype judgements. The fixed-prefix + trailing-`Vararg` normal form is
# compared element-by-element, with the trailing slots absorbed by the vararg
# element type. This table mirrors upstream `julia/src/subtype.c`
# (`subtype_tuple` / `subtype_tuple_varargs`).
#
# Notably this also covers the gap where bare `Tuple` (definitionally
# `Tuple{Vararg{Any}}`) was NOT recognised as a subtype of the universal
# vararg tuple `Tuple{Vararg{Any}}`.

using Test

@testset "fixed tuple <: trailing Vararg (Issue #5061)" begin
    @test Tuple{Int,Int} <: Tuple{Int,Vararg{Int}}
    @test Tuple{Int} <: Tuple{Int,Vararg{Int}}
    @test Tuple{Int,Int,Int} <: Tuple{Int,Vararg{Int}}
    @test Tuple{Int} <: Tuple{Vararg{Int}}
    @test Tuple{} <: Tuple{Vararg{Int}}
    @test Tuple{Int,Int} <: Tuple{Vararg{Int}}
    # element type widens under covariance
    @test Tuple{Int,Float64} <: Tuple{Int,Vararg{Real}}
    @test Tuple{Int,Int,Int,Int} <: Tuple{Int,Vararg{Integer}}
    @test Tuple{Int,Vararg{Int}} <: Tuple{Vararg{Integer}}
end

@testset "non-subtype tuple/Vararg cases (Issue #5061)" begin
    @test !(Tuple{Int,String} <: Tuple{Int,Vararg{Int}})
    @test !(Tuple{String} <: Tuple{Int,Vararg{Int}})
    @test !(Tuple{Int,Int,String} <: Tuple{Int,Vararg{Integer}})
    # a Vararg LHS may be empty, so it is not <: a fixed-arity tuple
    @test !(Tuple{Int,Vararg{Int}} <: Tuple{Int,Int})
    @test !(Tuple{Vararg{Int}} <: Tuple{Int,Vararg{Int}})
    # element type cannot narrow
    @test !(Tuple{Vararg{Real}} <: Tuple{Vararg{Int}})
    @test !(Tuple{Int,Vararg{Real}} <: Tuple{Int,Vararg{Int}})
    @test !(Tuple{Number,Vararg{Int}} <: Tuple{Real,Vararg{Int}})
end

@testset "Vararg{T,N} fixed-length tuples (Issue #5061)" begin
    @test Tuple{Int,Int,Int} <: Tuple{Vararg{Int,3}}
    @test !(Tuple{Int,Int} <: Tuple{Vararg{Int,3}})
    @test NTuple{3,Int} <: Tuple{Vararg{Int}}
end

@testset "bare Tuple === Tuple{Vararg{Any}} (Issue #5061)" begin
    # The bare `Tuple` datatype is the universal vararg tuple.
    @test Tuple <: Tuple{Vararg{Any}}
    @test Tuple{Vararg{Any}} <: Tuple
    @test Tuple{Int,Vararg{Int}} <: Tuple
    @test !(Tuple <: Tuple{Vararg{Int}})
    @test !(Tuple <: Tuple{Vararg{Real}})
    @test !(Tuple <: Tuple{Any})
    @test !(Tuple <: Tuple{Any,Vararg{Any}})
    @test !(Tuple <: Tuple{Int,Vararg{Int}})
end

@testset "tuple/Vararg typeintersect (Issue #5061)" begin
    @test typeintersect(Tuple{Int,Vararg{Int}}, Tuple{Vararg{Integer}}) ==
          Tuple{Int,Vararg{Int}}
    @test typeintersect(Tuple{Vararg{Int}}, Tuple{Int,Int}) == Tuple{Int,Int}
    @test typeintersect(Tuple{Int,Vararg{Real}}, Tuple{Vararg{Int}}) ==
          Tuple{Int,Vararg{Int}}
end

@testset "isa over Tuple{Int, Vararg{T}} (Issue #5061)" begin
    @test (1, 2, 3) isa Tuple{Int,Vararg{Int}}
    @test (1,) isa Tuple{Int,Vararg{Int}}
    @test !((1, "a") isa Tuple{Int,Vararg{Int}})
end

true
