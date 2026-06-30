# Test @nospecialize / @specialize in argument position of short-form
# function definitions: f(@nospecialize(x)) = ... (Issue #5122).
#
# Upstream Julia accepts @nospecialize(x) (and @specialize(x)) as an argument
# annotation that suppresses type specialization while the parameter still binds
# the value with its declared type. SubsetJuliaVM has no JIT/specialization, so
# the annotation is a no-op that must simply pass the argument through.
#
# The full-form `function f(@nospecialize x) ... end` already worked; this
# fixture covers the short-form `f(@nospecialize(x)) = expr` path, including the
# type-annotated `@nospecialize(x::T)` form and a leading nospecialized argument
# followed by a typed one.

using Test

# Bare nospecialized argument, short form.
f_short_5122(@nospecialize(x)) = x + 1

# Nospecialized argument with a declared type.
g_short_5122(@nospecialize(x::Number)) = x * 2

# Leading nospecialized argument followed by an ordinary typed parameter.
h_short_5122(@nospecialize(x), y::Int) = (x, y)

# @specialize in argument position is also accepted (and is a no-op here).
k_short_5122(@specialize(x)) = x - 1

@testset "@nospecialize argument annotation (short form)" begin
    # Same definition is reused for different argument types without error
    # (no per-type re-specialization is observable; the value passes through).
    @test f_short_5122(2) == 3
    @test f_short_5122(2.5) == 3.5

    @test g_short_5122(4) == 8
    @test g_short_5122(4.0) == 8.0

    @test h_short_5122("a", 3) == ("a", 3)
    @test h_short_5122(1.0, 5) == (1.0, 5)

    @test k_short_5122(10) == 9
end

true
