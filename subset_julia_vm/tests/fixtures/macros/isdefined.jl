# Test @isdefined macro (Issue #451)
# - Check if variable is defined in current scope
# - Works with undefined variables
# - Works inside functions

using Test

# Function definition must be outside @testset
function test_local_isdefined()
    a = 10
    result1 = @isdefined(a)
    result2 = @isdefined(nonexistent)
    (result1, result2)
end

@testset "@isdefined basic" begin
    # Undefined variable - should be false
    @test !@isdefined(undefined_var)

    # Defined variable - should be true
    x = 42
    @test @isdefined(x)

    # After assignment - should be true
    y = 0
    @test @isdefined(y)
end

@testset "@isdefined in function" begin
    result = test_local_isdefined()
    @test result[1]   # a is defined
    @test !result[2]  # nonexistent is not defined
end

@testset "@isdefined with global" begin
    global_var = 100
    @test @isdefined(global_var)
end

true
