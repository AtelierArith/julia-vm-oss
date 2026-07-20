using Test

@testset "hash determinism" begin
    @test hash(42) == hash(42)
    @test hash("hello") == hash("hello")
    @test hash(3.14) == hash(3.14)
end

@testset "hash type" begin
    @test isa(hash(42), Integer)
    @test isa(hash("test"), Integer)
end

@testset "hash special values" begin
    @test isa(hash(true), Integer)
    @test isa(hash(nothing), Integer)
end

true
