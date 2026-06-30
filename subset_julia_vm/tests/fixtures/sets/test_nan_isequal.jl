using Test

# Regression tests for Issues #3582, #3583, #3584:
# unique / allunique / allequal must use isequal-style equality so that
# NaN equals NaN (Julia set semantics).

@testset "unique with NaN (#3582)" begin
    @test length(unique([NaN, NaN])) == 1
    @test length(unique([NaN, NaN, NaN])) == 1
    @test length(unique([NaN, 1.0, NaN])) == 2
    # ordinary cases still work
    @test length(unique([1, 2, 1])) == 2
    @test length(unique([1, 2, 3])) == 3
end

@testset "allunique with NaN (#3583)" begin
    @test allunique([NaN, NaN]) == false
    @test allunique([1.0, NaN]) == true
    @test allunique([NaN, 1.0, NaN]) == false
    # ordinary cases still work
    @test allunique([1, 2, 1]) == false
    @test allunique([1, 2, 3]) == true
end

@testset "allequal with NaN (#3584)" begin
    @test allequal([NaN, NaN]) == true
    @test allequal([NaN, NaN, NaN]) == true
    @test allequal([NaN, 1.0]) == false
    @test allequal([1.0, NaN]) == false
    # ordinary cases still work
    @test allequal([2, 2]) == true
    @test allequal([2, 3]) == false
    @test allequal(Int[]) == true
    @test allequal([42]) == true
end

true
