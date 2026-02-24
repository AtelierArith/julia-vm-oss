using Test

@testset "isunordered Pure Julia (Issue #2715)" begin
    # === NaN is unordered ===
    @test isunordered(NaN) == true

    # === Missing is unordered ===
    @test isunordered(missing) == true

    # === Normal values are ordered ===
    @test isunordered(1) == false
    @test isunordered(3.14) == false
    @test isunordered(0.0) == false
    @test isunordered(-0.0) == false
    @test isunordered(Inf) == false
    @test isunordered(-Inf) == false
    @test isunordered("hello") == false
    @test isunordered('a') == false
    @test isunordered(true) == false
    @test isunordered(false) == false
    @test isunordered(nothing) == false
end

true
