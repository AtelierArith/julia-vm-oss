using Test

@testset "hash tuples" begin
    @test hash((1, 2)) == hash((1, 2))
    @test hash((1, 2)) != hash((2, 1))
    @test isa(hash((1, "two", 3.0)), UInt64)
end

@testset "hash equality contract" begin
    # Equal values must have equal hashes
    @test hash(1) == hash(1.0)
    @test hash(0) == hash(0.0)
    @test hash(0) == hash(false)
    @test hash(1) == hash(true)
end

@testset "hash distinct values" begin
    @test hash(1) != hash(2)
    @test hash("a") != hash("b")
    @test hash(1) != hash("1")
end

true
