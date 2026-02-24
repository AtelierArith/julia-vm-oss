# Test nonmissingtype function (Issue #1316)
# Returns the type with Missing removed from Union

using Test

@testset "nonmissingtype basic" begin
    # Non-Missing type returns unchanged
    @test nonmissingtype(Int64) === Int64
    @test nonmissingtype(Float64) === Float64
    @test nonmissingtype(String) === String

    # Missing type returns Union{} (Bottom)
    result = nonmissingtype(Missing)
    @test string(result) == "Union{}"

    # Any type returns unchanged (Any contains everything)
    @test nonmissingtype(Any) === Any
end

@testset "nonmissingtype with Union" begin
    # Union{Int64, Missing} -> Int64
    result1 = nonmissingtype(Union{Int64, Missing})
    @test result1 === Int64

    # Union{String, Missing} -> String
    result2 = nonmissingtype(Union{String, Missing})
    @test result2 === String

    # Union{Float64, Missing} -> Float64
    result3 = nonmissingtype(Union{Float64, Missing})
    @test result3 === Float64
end

true
