# collect(string) - collect string into character array (Issue #2027)

using Test

@testset "collect(string) into Char array (Issue #2027)" begin
    # Basic string collection
    result = collect("abc")
    @test length(result) == 3
    @test result[1] == 'a'
    @test result[2] == 'b'
    @test result[3] == 'c'

    # Longer string
    result2 = collect("hello")
    @test length(result2) == 5
    @test result2[1] == 'h'
    @test result2[5] == 'o'

    # Single character string
    result3 = collect("x")
    @test length(result3) == 1
    @test result3[1] == 'x'

    # Empty string
    result4 = collect("")
    @test length(result4) == 0

    # String with spaces
    result5 = collect("a b")
    @test length(result5) == 3
    @test result5[1] == 'a'
    @test result5[2] == ' '
    @test result5[3] == 'b'
end

true
