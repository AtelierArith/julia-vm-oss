using Test

@testset "hash determinism" begin
    @test hash(42) == hash(42)
    @test hash("hello") == hash("hello")
    @test hash(3.14) == hash(3.14)
end

@testset "hash type" begin
    @test isa(hash(42), UInt64)
    @test isa(hash("test"), UInt64)
end

@testset "hash negative zero" begin
    @test hash(0.0) == hash(-0.0)
end

@testset "hash special values" begin
    @test isa(hash(true), UInt64)
    @test isa(hash(nothing), UInt64)
end

true
