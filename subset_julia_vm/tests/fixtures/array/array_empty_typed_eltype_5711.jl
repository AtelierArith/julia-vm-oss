using Test

# Issue #5711: the element type of an EMPTY typed array literal was dropped for
# several types — eltype(Symbol[]) and eltype(Regex[]) returned Any instead of the
# declared element type (String[]/Char[]/Int[]/Pair[] already worked). The empty
# literal `T[]` lowers to Expr::TypedEmptyArray, whose element-type match omitted
# Symbol / Regex / RegexMatch and fell through to Any.

@testset "empty typed array literal element type (Issue #5711)" begin
    @test eltype(Symbol[]) == Symbol
    @test eltype(Regex[]) == Regex
    @test eltype(RegexMatch[]) == RegexMatch
    @test typeof(Symbol[]) == Vector{Symbol}
    @test typeof(Regex[]) == Vector{Regex}

    # Still-correct control cases (no regression).
    @test eltype(String[]) == String
    @test eltype(Char[]) == Char
    @test eltype(Int[]) == Int
    @test typeof(Float64[]) == Vector{Float64}

    # Non-empty literals unaffected.
    @test eltype(Symbol[:a, :b]) == Symbol
    @test eltype(Regex[r"a"]) == Regex

    # push! preserves the declared element type.
    w = Symbol[]
    push!(w, :x); push!(w, :y)
    @test w == [:x, :y]
    @test eltype(w) == Symbol

    r = Regex[]
    push!(r, r"\d+")
    @test eltype(r) == Regex
    @test occursin(r[1], "a9b") == true
end

true
