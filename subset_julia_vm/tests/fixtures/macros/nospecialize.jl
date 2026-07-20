# Test @nospecialize macro (Issues #2528 / #10237 / #10323)
# @nospecialize is a compiler hint: it must not change runtime semantics in
# signature or statement position. In value position, upstream
# `Base.@nospecialize`/`@specialize` always expand to
# `Expr(:meta, :nospecialize, vars...)` (base/essentials.jl), which evaluates
# to `nothing` at runtime without evaluating the wrapped argument (Issue
# #10323).

using Test

@testset "@nospecialize in function signature" begin
    f(@nospecialize(x)) = x
    @test f(42) == 42

    g(a, @nospecialize(b)) = a + b
    @test g(1, 2) == 3
end

@testset "@nospecialize statement position" begin
    function h(x, y)
        @nospecialize x y
        x * y
    end
    @test h(3, 4) == 12
end

@testset "@nospecialize in long-form function definition" begin
    function double(@nospecialize(x))
        return x * 2
    end
    @test double(5) == 10
end

@testset "@nospecialize/@specialize in value position (Issue #10323)" begin
    x = @nospecialize(42)
    @test x === nothing

    y = @specialize(7)
    @test y === nothing

    # No-argument form is also a meta expression that evaluates to nothing.
    @test @nospecialize() === nothing

    # The wrapped expression is never evaluated -- it is compiler metadata,
    # not a value to compute (matches upstream: `f()` is not called).
    calls = Ref(0)
    function bump()
        calls[] += 1
        return 99
    end
    z = @nospecialize(bump())
    @test z === nothing
    @test calls[] == 0
end

true
