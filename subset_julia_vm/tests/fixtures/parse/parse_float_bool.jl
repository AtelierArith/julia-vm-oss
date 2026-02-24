using Test

@testset "parse Float64" begin
    @test parse(Float64, "3.14") == 3.14
    @test parse(Float64, "-2.5") == -2.5
    @test parse(Float64, "0.0") == 0.0
    @test parse(Float64, "1e3") == 1000.0
end

@testset "tryparse Float64" begin
    @test tryparse(Float64, "2.71") == 2.71
    @test tryparse(Float64, "abc") === nothing
end

@testset "parse Int with bases" begin
    @test parse(Int64, "ff", base=16) == 255
    @test parse(Int64, "11", base=2) == 3
    @test parse(Int64, "77", base=8) == 63
end

@testset "tryparse edge cases" begin
    @test tryparse(Int64, "999999999999999999") == 999999999999999999
    @test tryparse(Int64, "  ") === nothing
    @test tryparse(Float64, "Inf") == Inf
    @test tryparse(Float64, "-Inf") == -Inf
end

true
