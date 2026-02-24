using Test

# Issue #3129: Operator partial application edge cases
# Tests ==(val), <(val), +(val) style partial application

@testset "Equality partial application" begin
    eq5 = ==(5)
    @test eq5(5) == true
    @test eq5(3) == false
    @test eq5(5.0) == true
end

@testset "Comparison partial application" begin
    lt10 = <(10)
    @test lt10(5) == true
    @test lt10(10) == false
    @test lt10(15) == false

    gt0 = >(0)
    @test gt0(5) == true
    @test gt0(0) == false
    @test gt0(-1) == false

    le5 = <=(5)
    @test le5(5) == true
    @test le5(4) == true
    @test le5(6) == false

    ge3 = >=(3)
    @test ge3(3) == true
    @test ge3(4) == true
    @test ge3(2) == false
end

@testset "Not-equal partial application" begin
    ne0 = !=(0)
    @test ne0(1) == true
    @test ne0(0) == false
end

@testset "Partial apply with filter" begin
    arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    @test filter(>(5), arr) == [6, 7, 8, 9, 10]
    @test filter(<(4), arr) == [1, 2, 3]
    @test filter(!=(5), arr) == [1, 2, 3, 4, 6, 7, 8, 9, 10]
end

@testset "Partial apply with any/all" begin
    arr = [2, 4, 6, 8]
    @test all(>(0), arr) == true
    @test any(==(4), arr) == true
    @test any(==(5), arr) == false
end

true
