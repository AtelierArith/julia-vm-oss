using Test

@testset "Channel do-block producer basic" begin
    c = Channel(10) do ch
        put!(ch, 1)
        put!(ch, 2)
        put!(ch, 3)
        put!(ch, 4)
        put!(ch, 5)
    end

    @test isopen(c) == false
    @test length(c) == 5
    @test take!(c) == 1
    @test take!(c) == 2
    @test take!(c) == 3
end

@testset "Channel do-block producer take in order" begin
    c = Channel(10) do ch
        put!(ch, 10)
        put!(ch, 20)
        put!(ch, 30)
    end

    @test take!(c) == 10
    @test take!(c) == 20
    @test take!(c) == 30
    @test isempty(c)
    @test isopen(c) == false
end

@testset "Channel do-block producer with Inf size" begin
    c = Channel(Inf) do ch
        put!(ch, 1)
        put!(ch, 4)
        put!(ch, 9)
    end

    @test isopen(c) == false
    @test take!(c) == 1
    @test take!(c) == 4
    @test take!(c) == 9
end

@testset "Channel do-block producer that errors propagates exception (Issue #3455)" begin
    @test_throws ErrorException Channel(10) do ch
        put!(ch, 1)
        error("producer error")
    end
end

@testset "empty! removes all channel items" begin
    c = Channel(10)
    put!(c, 1)
    put!(c, 2)
    put!(c, 3)
    @test length(c) == 3

    empty!(c)
    @test length(c) == 0
    @test isempty(c) == true
    @test isopen(c) == true
end

true
