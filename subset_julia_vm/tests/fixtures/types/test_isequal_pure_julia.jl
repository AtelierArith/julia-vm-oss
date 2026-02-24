using Test

@testset "isequal Pure Julia (Issue #2718)" begin
    # === Float64 specialization: uses === (bit identity) ===
    # NaN === NaN is true
    @test isequal(NaN, NaN) == true
    # -0.0 === 0.0 is false (different bit patterns)
    @test isequal(-0.0, 0.0) == false
    @test isequal(0.0, -0.0) == false
    # Normal float equality
    @test isequal(1.0, 1.0) == true
    @test isequal(1.0, 2.0) == false
    # Inf
    @test isequal(Inf, Inf) == true
    @test isequal(-Inf, -Inf) == true
    @test isequal(Inf, -Inf) == false

    # === Cross-type Int64/Float64 ===
    @test isequal(1, 1.0) == true
    @test isequal(1.0, 1) == true
    @test isequal(0, -0.0) == false
    @test isequal(-0.0, 0) == false
    @test isequal(2, 3.0) == false

    # === Int64 equality ===
    @test isequal(42, 42) == true
    @test isequal(42, 43) == false

    # === String equality ===
    @test isequal("hello", "hello") == true
    @test isequal("hello", "world") == false

    # === Char equality ===
    @test isequal('a', 'a') == true
    @test isequal('a', 'b') == false

    # === Nothing equality ===
    @test isequal(nothing, nothing) == true

    # === Missing specializations ===
    @test isequal(missing, missing) == true
    @test isequal(missing, 1) == false
    @test isequal(1, missing) == false
    @test isequal(missing, NaN) == false
    @test isequal(NaN, missing) == false

    # === Array specialization: element-wise with shape check ===
    @test isequal([1, 2, 3], [1, 2, 3]) == true
    @test isequal([1, 2, 3], [1, 2, 4]) == false
    @test isequal([1, 2], [1, 2, 3]) == false
    # NaN in arrays
    @test isequal([NaN, 1.0], [NaN, 1.0]) == true
    @test isequal([NaN, 1.0], [NaN, 2.0]) == false
    # -0.0 in arrays
    @test isequal([0.0], [-0.0]) == false

    # === Tuple specialization: element-wise ===
    @test isequal((1, 2), (1, 2)) == true
    @test isequal((1, 2), (1, 3)) == false
    @test isequal((1, 2), (1, 2, 3)) == false
    # NaN in tuples
    @test isequal((NaN,), (NaN,)) == true
    @test isequal((1, NaN), (1, NaN)) == true
    @test isequal((1, NaN), (2, NaN)) == false

    # === Bool equality ===
    @test isequal(true, true) == true
    @test isequal(true, false) == false
end

true
