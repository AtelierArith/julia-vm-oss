# Multi-variable array comprehension (Issue #2143)
# Tests [expr for var1 in iter1, var2 in iter2, ...] syntax
# Julia uses column-major order: first index (i) varies fastest

using Test

@testset "Multi-variable comprehension" begin
    # Basic two-variable comprehension (column-major: i varies fastest)
    result = [i * j for i in 1:3, j in 1:4]
    @test length(result) == 12
    # Column-major order: (1,1),(2,1),(3,1),(1,2),(2,2),(3,2),(1,3),(2,3),(3,3),(1,4),(2,4),(3,4)
    @test result[1] == 1   # 1*1
    @test result[2] == 2   # 2*1
    @test result[3] == 3   # 3*1
    @test result[4] == 2   # 1*2
    @test result[5] == 4   # 2*2
    @test result[6] == 6   # 3*2
    @test result[9] == 9   # 3*3
    @test result[12] == 12 # 3*4

    # Two-variable comprehension with addition
    sums = [i + j for i in 1:2, j in 1:3]
    @test length(sums) == 6
    # Column-major: (1,1),(2,1),(1,2),(2,2),(1,3),(2,3)
    @test sums[1] == 2  # 1+1
    @test sums[2] == 3  # 2+1
    @test sums[3] == 3  # 1+2
    @test sums[4] == 4  # 2+2
    @test sums[5] == 4  # 1+3
    @test sums[6] == 5  # 2+3

    # Three-variable comprehension
    result3 = [i + j + k for i in 1:2, j in 1:2, k in 1:2]
    @test length(result3) == 8
    # Column-major: i fastest, then j, then k
    @test result3[1] == 3  # 1+1+1
    @test result3[2] == 4  # 2+1+1
    @test result3[3] == 4  # 1+2+1
    @test result3[4] == 5  # 2+2+1
    @test result3[5] == 4  # 1+1+2
    @test result3[6] == 5  # 2+1+2
    @test result3[7] == 5  # 1+2+2
    @test result3[8] == 6  # 2+2+2
end

true
