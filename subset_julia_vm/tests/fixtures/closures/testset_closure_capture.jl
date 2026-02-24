# Test that closures defined INSIDE @testset blocks can capture outer variables (Issue #2358)
# Prior to the fix, closures at module-level (including @testset blocks) did not
# perform free variable analysis, causing "Undefined variable" errors.

using Test

@testset "Closures capturing variables inside @testset (Issue #2358)" begin
    # Basic capture test
    x = 10
    f = () -> x + 1
    @test f() == 11

    # Capture with modification
    y = 5
    g = () -> begin
        y * 2
    end
    @test g() == 10

    # Multiple captures
    a = 1
    b = 2
    c = 3
    h = () -> a + b + c
    @test h() == 6

    # Closure with parameter capturing outer variable
    outer_val = 100
    adder = n -> n + outer_val
    @test adder(5) == 105

    # Closure with complex expression
    base = 10
    multiplier = 2
    product = () -> base * multiplier
    @test product() == 20
end

@testset "Closure mutation capture inside @testset (Issue #2358)" begin
    # Mutable capture (reading mutable value)
    counter = 0
    get_counter = () -> counter
    @test get_counter() == 0
    counter = 5
    @test get_counter() == 5

    # Array capture (common pattern)
    arr = [1, 2, 3]
    sum_arr = () -> arr[1] + arr[2] + arr[3]
    @test sum_arr() == 6
    arr[1] = 10
    @test sum_arr() == 15
end

true
