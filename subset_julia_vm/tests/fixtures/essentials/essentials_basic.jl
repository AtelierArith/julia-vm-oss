using Test

@testset "ifelse" begin
    @test ifelse(true, 10, 20) == 10
    @test ifelse(false, 10, 20) == 20
    @test ifelse(1 > 0, "yes", "no") == "yes"
    @test ifelse(1 < 0, "yes", "no") == "no"
end

true
