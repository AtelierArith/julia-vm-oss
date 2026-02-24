# Test Test module macros: @test, @testset, @test_throws, @test_broken (Issue #2864)
#
# The Test module provides unit testing macros. This fixture verifies that
# all supported macros work correctly in the SubsetJuliaVM.

using Test

@testset "@test basic assertions" begin
    @test true
    @test 1 + 1 == 2
    @test 3 * 4 == 12
    @test "hello" == "hello"
    @test length("abc") == 3
end

@testset "@test with expressions" begin
    x = 42
    @test x == 42
    @test x > 0
    @test x < 100
    @test x != 0
end

@testset "@test_throws exceptions" begin
    @test_throws ErrorException error("oops")
    @test_throws BoundsError getindex([1, 2, 3], 10)
end

@testset "@test_broken expected failures" begin
    @test_broken 1 == 2
    @test_broken false
end

@testset "nested testsets" begin
    @testset "arithmetic" begin
        @test 2 + 2 == 4
        @test 10 - 3 == 7
    end
    @testset "strings" begin
        @test string("a", "b") == "ab"
        @test length("hello") == 5
    end
end

true
