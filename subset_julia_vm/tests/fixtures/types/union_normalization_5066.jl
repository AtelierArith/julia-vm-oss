# Issue #5066: deep nested Union normalization (flatten / dedup / sort / collapse)
# Equal Unions share one canonical normal form, so `===` is independent of
# nesting depth, member order, and duplicates — matching upstream Julia's
# `jl_type_union` (julia/src/jltypes.c).

using Test

@testset "Union normalization: flatten / dedup / sort / collapse (Issue #5066)" begin
    # Flatten nested unions
    @test Union{Int, Union{Float64, Int}} === Union{Int, Float64}
    @test Union{Int, Union{Float64, String}} === Union{Int, Float64, String}
    @test Union{Union{Int, Float64}, Union{String, Char}} === Union{Int, Float64, String, Char}

    # Order-independent identity (canonical sort)
    @test Union{Int, Float64} === Union{Float64, Int}
    @test Union{String, Int, Float64} === Union{Float64, Int, String}

    # Singleton collapse: a one-element union is the element itself
    @test Union{Int} === Int
    @test Union{String} === String

    # Bottom (empty union)
    @test Union{} === Union{}
    @test Union{Union{}, Int} === Int

    # Duplicate removal
    @test Union{Int, Int} === Int
    @test Union{Int, Float64, Int} === Union{Int, Float64}

    # Subtype absorption (A <: B removes A)
    @test Union{Int, Integer} === Integer
    @test Union{Int8, Int16, Integer} === Integer
    @test Union{Int, Real, Float64} === Real
    @test Union{Int, Any} === Any

    # Nested + duplicate + reorder all at once
    @test Union{String, Union{Int, String}, Int} === Union{Int, String}
end

@testset "Union canonical display order (Issue #5066)" begin
    # singleton < isbits < other DataType < non-DataType; ties break by name
    @test string(Union{Int, Float64}) == "Union{Float64, Int64}"
    @test string(Union{String, Int}) == "Union{Int64, String}"
    @test string(Union{Nothing, Int, Missing}) == "Union{Missing, Nothing, Int64}"
    @test string(Union{Char, String, Symbol}) == "Union{Char, String, Symbol}"
    @test string(Union{Int128, Int16, Int32, Int64, Int8}) ==
          "Union{Int128, Int16, Int32, Int64, Int8}"
end

true
