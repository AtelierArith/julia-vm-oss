# Test fill() with non-numeric types (Issue #2177)
# Julia: fill("hello", 3) returns ["hello", "hello", "hello"]

using Test

@testset "fill with String" begin
    a = fill("hello", 3)
    @test length(a) == 3
    @test a[1] == "hello"
    @test a[2] == "hello"
    @test a[3] == "hello"
end

@testset "fill with String - different values" begin
    a = fill("world", 4)
    @test length(a) == 4
    @test a[1] == "world"
    @test a[4] == "world"
end

@testset "fill with Bool" begin
    a = fill(true, 3)
    @test length(a) == 3
    @test a[1] == true
    @test a[2] == true
    @test a[3] == true
end

@testset "fill with Int64 (regression)" begin
    a = fill(42, 3)
    @test length(a) == 3
    @test a[1] == 42
    @test a[2] == 42
    @test a[3] == 42
end

@testset "fill with Float64 (regression)" begin
    a = fill(3.14, 3)
    @test length(a) == 3
    @test a[1] == 3.14
    @test a[2] == 3.14
end

true
