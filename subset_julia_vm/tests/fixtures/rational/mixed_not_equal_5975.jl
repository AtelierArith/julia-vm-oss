using Test

# Issue #5975: `!=` between a Rational and a Real (Integer / AbstractFloat), in
# either order, used to raise a MethodError even though the matching `==` works.
# Upstream Julia has `!=(x, y) = !(x == y)`; these explicit mixed methods restore
# that for the Rational pairs the generic fallback did not reach.

@testset "Rational != Integer (Issue #5975)" begin
    @test (1//2 != 1) == true
    @test (1 != 1//2) == true
    @test (2//2 != 1) == false
    @test (1 != 2//2) == false
    @test (1//1 != 1) == false
    # consistency with ==
    @test (1//2 != 1) == !(1//2 == 1)
    @test (2//2 != 1) == !(2//2 == 1)
end

@testset "Rational != AbstractFloat (Issue #5975)" begin
    @test (1//2 != 0.5) == false
    @test (0.5 != 1//2) == false
    @test (1//3 != 0.5) == true
    @test (0.5 != 1//3) == true
    @test (1//2 != 0.5) == !(1//2 == 0.5)
end

# Pairs that already worked must keep working (no dispatch regression).
@testset "Rational != Rational / Complex still work (Issue #5975)" begin
    @test (1//2 != 1//3) == true
    @test (1//2 != 1//2) == false
    @test (1//1 != Complex(1, 0)) == false
    @test (Complex(1, 0) != 1//1) == false
    @test (1//1 != Complex(1, 2)) == true
end

true
