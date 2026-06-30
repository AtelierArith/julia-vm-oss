using Test

@testset "repr of typed empty vectors preserves element type (Issue #4733)" begin
    @test repr(Int[]) == "Int64[]"
    @test repr(Int64[]) == "Int64[]"
    @test repr(Float64[]) == "Float64[]"
    @test repr(Any[]) == "Any[]"
    @test repr(String[]) == "String[]"
    @test repr(Bool[]) == "Bool[]"

    # Non-empty cases still use the inline bracket form from #4731.
    @test repr([1, 2, 3]) == "[1, 2, 3]"
    @test repr(["a"]) == "[\"a\"]"
end

@testset "string of typed empty vectors agrees with repr (Issue #4733)" begin
    # string() uses the same compact path, so it must agree with repr
    # for empty typed arrays — both should be "<eltype>[]".
    @test string(Int[]) == "Int64[]"
    @test string(Float64[]) == "Float64[]"
end

true
