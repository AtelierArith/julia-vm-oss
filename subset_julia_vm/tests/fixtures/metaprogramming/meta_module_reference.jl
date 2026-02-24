# Test that Meta module can be referenced directly (not just Meta.func())
# This ensures Meta is recognized as a module literal

using Test

@testset "Meta module can be referenced directly (typeof(Meta), m = Meta)" begin

    # typeof(Meta) should return Module
    @assert typeof(Meta) == Module

    # Meta should be usable as a value
    m = Meta
    @assert typeof(m) == Module

    # Meta.parse should work
    expr = Meta.parse("1+1")
    @assert Meta.isexpr(expr, :call)

    # Meta.parse with more complex expressions
    expr2 = Meta.parse("f(x, y)")
    @assert expr2.head == :call

    @test (true)
end

true  # Test passed
