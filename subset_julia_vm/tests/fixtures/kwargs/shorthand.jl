# Test: Keyword argument shorthand syntax
# Tests the shorthand syntax where g(;a, b) expands to g(;a=a, b=b)

using Test

# Function with required and default keyword arguments
g(; a, b=10) = a + b

# Function with multiple required kwargs
h(; x, y, z) = x + y + z

# Function with all default kwargs
k(; a=1, b=2, c=3) = a * b * c

@testset "Keyword argument shorthand syntax" begin
    # Test shorthand with required + default kwargs
    a = 3
    b = 4
    @test g(;a, b) == 7  # g(;a=a, b=b) = 3 + 4

    # Test shorthand with only required kwarg (default for b)
    @test g(;a) == 13  # g(;a=a) = 3 + 10

    # Test shorthand with multiple required kwargs
    x = 10
    y = 20
    z = 30
    @test h(;x, y, z) == 60  # h(;x=x, y=y, z=z) = 10 + 20 + 30

    # Test shorthand with partial override of defaults
    c = 5
    @test k(;c) == 10  # k(;c=c) = 1 * 2 * 5

    # Test mixed shorthand and explicit
    @test g(;a, b=100) == 103  # a=3, b=100
    @test h(;x, y, z=1000) == 1030  # 10 + 20 + 1000
end

true
