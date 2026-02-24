# Test eltype function (Issue #2570)
# Verifies eltype works correctly for arrays and other types
using Test

@testset "eltype Pure Julia" begin
    # Array element types
    @test eltype([1, 2, 3]) == Int64
    @test eltype([1.0, 2.0, 3.0]) == Float64
    @test eltype([true, false]) == Bool

    # eltype returns DataType, verify with typeof
    @test typeof(eltype([1, 2, 3])) == DataType
end

true
