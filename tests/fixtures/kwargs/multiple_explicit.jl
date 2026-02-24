# Test: Multiple explicit keyword arguments in function definition
# Tests both required (no default) and default keyword arguments

using Test

# Function with required and default keyword arguments (short form)
g(; a, b=10) = a + b

# Function with multiple default kwargs (short form)
h(; x=1, y=2, z=3) = x + y + z

# Function with multiple required kwargs (short form)
k(; a, b) = a * b

# Function with multiple kwargs (long form)
function f(; a, b=10, c=100)
    return a + b + c
end

@testset "Multiple explicit keyword arguments" begin
    # Test required + default kwargs
    @test g(;a=5, b=20) == 25
    @test g(;a=5) == 15  # b uses default

    # Test multiple default kwargs
    @test h() == 6  # all defaults: 1+2+3
    @test h(;x=10) == 15  # 10+2+3
    @test h(;x=10, y=20, z=30) == 60

    # Test multiple required kwargs
    @test k(;a=3, b=4) == 12

    # Test long form function with mixed kwargs
    @test f(;a=1) == 111  # 1+10+100
    @test f(;a=1, b=20, c=300) == 321
end

true
