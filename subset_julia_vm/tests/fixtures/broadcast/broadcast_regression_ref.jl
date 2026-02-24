# Regression test for Ref() wrapping in dot-syntax broadcasts
# Tests that Ref() prevents broadcasting and treats value as scalar.
# Note: Ref() is supported in dot-call syntax (f.(args, Ref(x))),
# not in the broadcast() function form.
# Related: Issue #2550 (broadcast regression test suite)

using Test

@testset "Ref wrapping in dot-call broadcast" begin
    a = [1.0, 2.0, 3.0]

    # f.(array, Ref(scalar)) should broadcast array, keep scalar constant
    add(x, y) = x + y
    result = add.(a, Ref(10.0))
    @test result[1] == 11.0
    @test result[2] == 12.0
    @test result[3] == 13.0
end

@testset "Ref with binary operators" begin
    a = [1.0, 2.0, 3.0]

    # Using Ref() with dot-syntax binary operations
    mul(x, y) = x * y
    result = mul.(a, Ref(2.0))
    @test result == [2.0, 4.0, 6.0]
end

true
