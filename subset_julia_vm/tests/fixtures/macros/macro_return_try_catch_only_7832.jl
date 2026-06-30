using Test

macro catch_only_try()
    esc(:(try
        error("x")
    catch
        42
    end))
end

@testset "macro-returned catch-only try expressions lower" begin
    @test (@catch_only_try) == 42

    function catch_only_value()
        @catch_only_try
    end

    @test catch_only_value() == 42
end

true
