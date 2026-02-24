using Test

# Function composition using ∘ operator
double(x) = x * 2
increment(x) = x + 1

@testset "function composition with ∘" begin
    double_then_inc = increment ∘ double
    @test double_then_inc(3) == 7  # double(3)=6, increment(6)=7

    inc_then_double = double ∘ increment
    @test inc_then_double(3) == 8  # increment(3)=4, double(4)=8

    # Three-way composition
    square(x) = x^2
    composed = square ∘ increment ∘ double
    @test composed(2) == 25  # double(2)=4, increment(4)=5, square(5)=25

    # Using anonymous functions
    composed2 = (x -> x + 10) ∘ (x -> x * 3)
    @test composed2(2) == 16  # x*3=6, 6+10=16
end

true
