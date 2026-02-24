# Test calling a function stored in a struct field
# This tests the g.f(x) pattern where f is a Function field

using Test

# Define a simple function to store
add_one(x) = x + 1
double(x) = x * 2

# Struct with a Function field
struct Container
    f::Function
end

@testset "Field function call" begin
    # Basic case: call function stored in field
    c = Container(add_one)
    @test c.f(5) == 6
    @test c.f(10) == 11

    # Different function
    c2 = Container(double)
    @test c2.f(5) == 10
    @test c2.f(3) == 6
end

true
