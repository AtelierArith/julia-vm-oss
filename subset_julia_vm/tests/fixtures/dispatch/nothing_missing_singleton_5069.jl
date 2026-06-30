# Issue #5069: systematic type-system integration of the singleton types
# Nothing (`nothing`) and Missing (`missing`).
#
# Covers: singleton type identity, isa/subtype, Union{T,Nothing} (the Optional
# pattern) runtime dispatch f(::Nothing) vs f(::Int), the isnothing/ismissing
# predicates, something/coalesce, and basic Missing propagation. Matches upstream
# Julia exactly. Verified against `julia` before landing.

using Test

@testset "Nothing/Missing singleton identity" begin
    @test typeof(nothing) === Nothing
    @test typeof(missing) === Missing
    @test isa(nothing, Nothing)
    @test isa(missing, Missing)
    @test !isa(nothing, Missing)
    @test !isa(missing, Nothing)
end

@testset "Nothing/Missing subtype (post Issue #5257)" begin
    # Nothing/Missing are concrete singleton DataTypes, NOT the bottom type.
    @test !(Nothing <: Int64)
    @test !(Missing <: Int64)
    @test Nothing <: Any
    @test Missing <: Any
    @test Nothing <: Nothing
    @test Missing <: Missing
    @test Nothing <: Union{Int64,Nothing}
    @test Missing <: Union{Int64,Missing}
    @test !(Nothing <: Union{Int64,Float64})
end

@testset "isa with Union (Optional pattern membership)" begin
    @test nothing isa Union{Nothing,Int}
    @test missing isa Union{Missing,Int}
    @test !(nothing isa Union{Missing,Int})
    @test !(missing isa Union{Nothing,Int})
end

@testset "Runtime Union dispatch f(::Nothing) vs f(::Int)" begin
    f(::Nothing) = "got nothing"
    f(::Int) = "got int"
    @test f(nothing) == "got nothing"
    @test f(3) == "got int"

    # via a wrapper so the argument flows as a value, not a literal
    relay(x) = f(x)
    @test relay(nothing) == "got nothing"
    @test relay(5) == "got int"
end

@testset "Runtime Union dispatch g(::Missing) vs g(::Int)" begin
    g(::Missing) = "got missing"
    g(::Int) = "got int"
    @test g(missing) == "got missing"
    @test g(9) == "got int"
end

@testset "Optional pattern: Union{Int,Nothing} parameter" begin
    function opt(x::Union{Int,Nothing})
        if isnothing(x)
            return -1
        else
            return x + 100
        end
    end
    @test opt(nothing) == -1
    @test opt(5) == 105
end

@testset "isnothing / ismissing predicates" begin
    @test isnothing(nothing)
    @test !isnothing(1)
    @test !isnothing(missing)
    @test ismissing(missing)
    @test !ismissing(1)
    @test !ismissing(nothing)
end

@testset "something / coalesce narrowing" begin
    @test something(nothing, 1) == 1
    @test something(nothing, nothing, 3) == 3
    @test something(Some(7), 1) == 7
    # `missing` is a real value: something stops at the first non-nothing
    @test ismissing(something(missing, nothing, 3))

    @test coalesce(missing, 2) == 2
    @test coalesce(missing, missing, 7) == 7
    @test coalesce(1, 2) == 1
    @test ismissing(coalesce(missing, missing))
end

@testset "Missing propagation basics" begin
    @test ismissing(missing + 1)
    @test ismissing(1 + missing)
    @test ismissing(missing - missing)
    @test ismissing(missing * 2)
    @test ismissing(missing == 1)
    @test ismissing(missing < 2)
    @test (missing + 1) === missing
    # === is identity (Bool), not three-valued
    @test (missing === missing) == true
    @test (missing === 1) == false
    @test (nothing === nothing) == true
end

true
