# Test keyword argument shorthand syntax in function calls
# f(;x) is equivalent to f(;x=x)

using Test

# Define functions with keyword arguments
f(; x, y) = x * y
single(; val) = val * 2
result_fn(; m) = m

# Define test variables at top level
# (Variables used in shorthand syntax must be visible at compile time)
x = 3
y = 2
val = 7
m = 42

@testset "Keyword argument shorthand in function calls" begin
    # Basic shorthand: f(;x, y) where x and y are top-level variables
    @test f(;x, y) == 6

    # Single shorthand argument
    @test single(;val) == 14

    # Another shorthand test
    @test result_fn(;m) == 42
end

true
