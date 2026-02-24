# Test compose operator with closures (Issue #2298)
# Closures should be composable with ∘, just like regular functions.

using Test

# Helper to create closures (using inner function definitions)
function make_adder(n)
    function adder(x)
        x + n
    end
    adder
end

function make_multiplier(n)
    function multiplier(x)
        x * n
    end
    multiplier
end

# Named functions for mixed composition
double(x) = x * 2
inc(x) = x + 1

@testset "Closure ∘ Closure" begin
    add1 = make_adder(1)
    add2 = make_adder(2)
    composed = add2 ∘ add1
    # (add2 ∘ add1)(10) = add2(add1(10)) = add2(11) = 13
    @test composed(10) == 13
end

@testset "Function ∘ Closure" begin
    add3 = make_adder(3)
    composed = double ∘ add3
    # (double ∘ add3)(5) = double(add3(5)) = double(8) = 16
    @test composed(5) == 16
end

@testset "Closure ∘ Function" begin
    mul5 = make_multiplier(5)
    composed = mul5 ∘ inc
    # (mul5 ∘ inc)(3) = mul5(inc(3)) = mul5(4) = 20
    @test composed(3) == 20
end

@testset "Closure ∘ ComposedFunction" begin
    add10 = make_adder(10)
    # double ∘ inc is a ComposedFunction
    double_then_inc = double ∘ inc
    composed = add10 ∘ double_then_inc
    # (add10 ∘ double ∘ inc)(3) = add10(double(inc(3))) = add10(double(4)) = add10(8) = 18
    @test composed(3) == 18
end

@testset "ComposedFunction ∘ Closure" begin
    mul3 = make_multiplier(3)
    double_then_inc = double ∘ inc
    composed = double_then_inc ∘ mul3
    # (double ∘ inc ∘ mul3)(2) = double(inc(mul3(2))) = double(inc(6)) = double(7) = 14
    @test composed(2) == 14
end

true
