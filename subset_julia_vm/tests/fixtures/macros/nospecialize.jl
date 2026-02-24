# Test @nospecialize macro (Issue #2528)
# - @nospecialize is a no-op in SubsetJuliaVM (no type specialization)
# - @nospecialize(x) should pass through its argument unchanged
# - Used in broadcast.jl to control specialization

using Test

@testset "@nospecialize basic" begin
    # @nospecialize wrapping an expression should return the expression
    x = @nospecialize(42)
    @test x == 42
end

@testset "@nospecialize with typed argument" begin
    # @nospecialize wrapping a typed expression
    val = @nospecialize(1 + 2)
    @test val == 3
end

@testset "@nospecialize in function body" begin
    # @nospecialize used inside a function
    function f(x)
        y = @nospecialize(x)
        return y * 2
    end
    @test f(5) == 10
end

true
