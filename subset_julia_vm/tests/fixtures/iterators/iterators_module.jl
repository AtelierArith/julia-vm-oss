# Iterators module namespace support (Issue #2066)
# Iterators.flatten, Iterators.enumerate, etc. should work

using Test

@testset "Iterators.flatten" begin
    @test collect(Iterators.flatten([[1,2],[3,4]])) == [1,2,3,4]
    @test collect(Iterators.flatten([[1],[2,3],[4,5,6]])) == [1,2,3,4,5,6]
end

@testset "Iterators.enumerate" begin
    r = collect(Iterators.enumerate([10,20,30]))
    @test r[1] == (1, 10)
    @test r[2] == (2, 20)
    @test r[3] == (3, 30)
end

@testset "Iterators.zip" begin
    r = collect(Iterators.zip([1,2,3],[4,5,6]))
    @test r[1] == (1, 4)
    @test r[2] == (2, 5)
    @test r[3] == (3, 6)
end

@testset "Iterators.take" begin
    @test collect(Iterators.take(1:100, 5)) == [1,2,3,4,5]
end

@testset "Iterators.drop" begin
    @test collect(Iterators.drop(1:10, 7)) == [8,9,10]
end

@testset "Iterators.repeated" begin
    @test collect(Iterators.repeated(42, 3)) == [42,42,42]
end

@testset "Iterators.countfrom" begin
    r = collect(Iterators.take(Iterators.countfrom(5), 4))
    @test r == [5,6,7,8]
end

@testset "Iterators.cycle" begin
    r = collect(Iterators.take(Iterators.cycle([1,2,3]), 7))
    @test r == [1,2,3,1,2,3,1]
end

true
