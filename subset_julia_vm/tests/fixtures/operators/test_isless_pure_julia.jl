using Test

@testset "isless Pure Julia (Issue #2712)" begin
    # === Integer comparison ===
    @test isless(1, 2) == true
    @test isless(2, 1) == false
    @test isless(1, 1) == false
    @test isless(-1, 0) == true

    # === Float comparison ===
    @test isless(1.0, 2.0) == true
    @test isless(2.0, 1.0) == false
    @test isless(1.0, 1.0) == false

    # === NaN handling ===
    @test isless(NaN, 1.0) == false    # NaN is NOT less than anything
    @test isless(1.0, NaN) == true     # everything is less than NaN
    @test isless(NaN, NaN) == false    # NaN is NOT less than NaN
    @test isless(NaN, Inf) == false    # NaN is NOT less than Inf
    @test isless(-Inf, NaN) == true    # -Inf is less than NaN

    # === Cross-type numeric comparison ===
    @test isless(1, 2.0) == true
    @test isless(2.0, 1) == false
    @test isless(1, 1.0) == false
    @test isless(1, NaN) == true

    # === String comparison ===
    @test isless("a", "b") == true
    @test isless("b", "a") == false
    @test isless("abc", "abd") == true
    @test isless("a", "a") == false

    # === Char comparison ===
    @test isless('a', 'b') == true
    @test isless('b', 'a') == false
    @test isless('a', 'a') == false

    # === Bool comparison ===
    @test isless(false, true) == true
    @test isless(true, false) == false
    @test isless(false, false) == false
    @test isless(true, true) == false

    # === Missing handling ===
    @test isless(missing, missing) == false
    @test isless(missing, 1) == false
    @test isless(1, missing) == true
    @test isless(missing, NaN) == false
    @test isless(NaN, missing) == true
end

true
