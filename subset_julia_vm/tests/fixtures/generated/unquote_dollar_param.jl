# Test: @generated unquote path supports `$ident` interpolation (Issue #5934)
#
# A `@generated` function whose quoted body interpolates a bound type/value
# parameter with `$N` must lower `$N` to the bound parameter `N`. Upstream
# Julia evaluates `:($N + 1)` to `N + 1` at staging time, so `g(Val(10))`
# returns 11.
#
# Before the fix, the unquote path lowered the quote's inner expression as
# plain code, so `$N` hit `map_unary_op("$") = None` and was rejected with
# `UnsupportedOperator("$")`.
#
# SCOPE: `$ident` only. `$(expr)` compound-paren / `$(esc(...))` / `$(p...)`
# splat are a separate follow-up (the real staging engine). Parity is claimed
# only for the `Val{N}`-style bound value/type parameter form.

using Test

# Short form: @generated f(...) = :(...)
@generated g(::Val{N}) where N = :($N + 1)

# Long form: @generated function ... return :(...) ... end
@generated function f(::Val{N}) where N
    return :($N + 1)
end

@testset "generated unquote dollar param (Issue #5934)" begin
    @test g(Val(10)) == 11
    @test g(Val(3)) == 4
    @test f(Val(10)) == 11
    @test f(Val(7)) == 8
end

# Final value is a boolean conjunction so nextest gates on the actual results
# (a bare `@test` does not abort sjulia on failure).
g(Val(10)) == 11 && g(Val(3)) == 4 && f(Val(10)) == 11 && f(Val(7)) == 8
