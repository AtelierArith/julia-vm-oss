# Test default function arguments (Issue #2090)
# Julia desugars f(a, b=10) into two methods:
#   f(a, b) - full method
#   f(a) = f(a, 10) - stub with default

using Test

# Full form: function ... end
function greet(name, greeting="Hello")
    string(greeting, ", ", name)
end

# Short form: f(x) = expr
add_with_default(a, b=10) = a + b

# Multiple defaults
function multi_default(a, b=2, c=3)
    a + b + c
end

@testset "Default function arguments" begin
    # Full form with default
    @test greet("Julia") == "Hello, Julia"
    @test greet("Julia", "Hi") == "Hi, Julia"

    # Short form with default
    @test add_with_default(5) == 15
    @test add_with_default(5, 20) == 25

    # Multiple defaults
    @test multi_default(1) == 6        # 1 + 2 + 3
    @test multi_default(1, 10) == 14   # 1 + 10 + 3
    @test multi_default(1, 10, 100) == 111  # 1 + 10 + 100
end

true
