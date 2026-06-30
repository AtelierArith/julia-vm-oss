# Test summary function
# summary(x) returns a string describing the type of x

using Test

@testset "summary function" begin
    # Basic type summary
    @test summary(42) == "Int64"
    @test summary(3.14) == "Float64"
    @test summary(:symbol) == "Symbol"
    @test summary(true) == "Bool"
    @test summary(nothing) == "Nothing"

    # Tuple summary
    @test summary((1, 2, 3)) == "Tuple{Int64, Int64, Int64}"

    # String summary (Issue #4706): "N-codeunit String" / "empty String"
    @test summary("hello") == "5-codeunit String"
    @test summary("abc") == "3-codeunit String"
    @test summary("") == "empty String"

    # Dict summary (Issue #4706): "Dict{K, V} with N entry/entries"
    @test summary(Dict("a" => 1)) == "Dict{String, Int64} with 1 entry"
    @test summary(Dict("a" => 1, "b" => 2)) == "Dict{String, Int64} with 2 entries"
    @test summary(Dict{String, Int}()) == "Dict{String, Int64} with 0 entries"

    # Array summary - vectors
    v = [1.0, 2.0, 3.0]
    s = summary(v)
    @test occursin("3-element", s)
    @test occursin("Vector", s) || occursin("Array", s)

    # Array summary - empty vector
    empty_v = Float64[]
    s_empty = summary(empty_v)
    @test occursin("0-element", s_empty)
end

true
