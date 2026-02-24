using Test

@testset "parse Int64" begin
    @test parse(Int64, "42") == 42
    @test parse(Int64, "-10") == -10
    @test parse(Int64, "+5") == 5
    @test parse(Int64, "0") == 0
end

@testset "tryparse Int64" begin
    @test tryparse(Int64, "42") == 42
    @test tryparse(Int64, "abc") === nothing
    @test tryparse(Int64, "") === nothing
end

true
