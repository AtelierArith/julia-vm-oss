# Test findmin! in-place function

using Test

@testset "findmin! function" begin
    # Test with integer array
    arr = [3, 1, 4, 1, 5, 9, 2, 6]
    rval = [0]
    rind = [0]
    result = findmin!(rval, rind, arr)
    @test rval[1] == 1
    @test rind[1] == 2
    @test result[1] === rval
    @test result[2] === rind

    # Test with float array
    arr2 = [1.5, 3.2, 2.8, 0.1]
    rval2 = [0.0]
    rind2 = [0]
    findmin!(rval2, rind2, arr2)
    @test rval2[1] == 0.1
    @test rind2[1] == 4

    # Test with negative numbers
    arr3 = [-5, -2, -8, -1]
    rval3 = [0]
    rind3 = [0]
    findmin!(rval3, rind3, arr3)
    @test rval3[1] == -8
    @test rind3[1] == 3

    # Test single element
    arr4 = [42]
    rval4 = [0]
    rind4 = [0]
    findmin!(rval4, rind4, arr4)
    @test rval4[1] == 42
    @test rind4[1] == 1
end

true
