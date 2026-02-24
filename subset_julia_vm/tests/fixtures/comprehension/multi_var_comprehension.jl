# Multi-variable array comprehension (Issue #2143)
# Tests [expr for i in R1, j in R2] producing flat array via cartesian product
# Julia uses column-major order: first index varies fastest

using Test

@testset "Multi-variable comprehension" begin
    # Basic 2-variable comprehension (column-major: i varies fastest)
    result = [i * j for i in 1:3, j in 1:3]
    @test length(result) == 9
    # Order: (1,1),(2,1),(3,1),(1,2),(2,2),(3,2),(1,3),(2,3),(3,3)
    @test result[1] == 1   # 1*1
    @test result[2] == 2   # 2*1
    @test result[3] == 3   # 3*1
    @test result[4] == 2   # 1*2
    @test result[5] == 4   # 2*2
    @test result[6] == 6   # 3*2
    @test result[7] == 3   # 1*3
    @test result[8] == 6   # 2*3
    @test result[9] == 9   # 3*3

    # Addition with different range sizes
    result2 = [i + j for i in 1:2, j in 1:3]
    @test length(result2) == 6
    # Order: (1,1),(2,1),(1,2),(2,2),(1,3),(2,3)
    @test result2[1] == 2  # 1+1
    @test result2[2] == 3  # 2+1
    @test result2[3] == 3  # 1+2
    @test result2[4] == 4  # 2+2
    @test result2[5] == 4  # 1+3
    @test result2[6] == 5  # 2+3

    # Body using only first variable (i varies fastest)
    result3 = [i for i in 1:3, j in 1:2]
    @test length(result3) == 6
    # Order: (1,1),(2,1),(3,1),(1,2),(2,2),(3,2)
    @test result3[1] == 1
    @test result3[2] == 2
    @test result3[3] == 3
    @test result3[4] == 1
    @test result3[5] == 2
    @test result3[6] == 3
end

true
