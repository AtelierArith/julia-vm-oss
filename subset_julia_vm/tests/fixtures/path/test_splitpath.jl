# Test splitpath function

using Test

@testset "splitpath function" begin
    # Simple relative path
    result = splitpath("a/b/c")
    @test length(result) == 3
    @test result[1] === "a"
    @test result[2] === "b"
    @test result[3] === "c"

    # Absolute path
    result2 = splitpath("/home/user")
    @test length(result2) == 3
    @test result2[1] === "/"
    @test result2[2] === "home"
    @test result2[3] === "user"

    # Empty path
    result3 = splitpath("")
    @test length(result3) == 1
    @test result3[1] === ""

    # Root path only
    result4 = splitpath("/")
    @test length(result4) == 1
    @test result4[1] === "/"

    # Single component
    result5 = splitpath("file.txt")
    @test length(result5) == 1
    @test result5[1] === "file.txt"
end

true
