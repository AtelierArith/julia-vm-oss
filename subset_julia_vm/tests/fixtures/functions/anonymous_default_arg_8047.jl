using Test

# Anonymous / arrow functions with optional positional default arguments
# (Issue #8047). Before the fix, `(x, d=2) -> ...` parsed but the default was
# never bound: the reduced-arity call raised `UndefVarError` (default param left
# unbound) and the full-arity call raised `NoMethodFound` (only a 1-arg lambda
# was created, no default-arg stub). Named/short/block forms already went through
# `generate_default_arg_stubs`; the arrow-lowering paths
# (`lower_arrow_function`, `lower_arrow_function_with_name`, the IIFE/nested
# variants) did not. This is the anonymous row deferred from the #8040
# (form × default-value node-kind) matrix.
#
# For every default-value node kind we assert BOTH the reduced-arity call
# (default applied) and the full-arity call (default overridden), and also cover
# multiple defaults, typed defaults, and the immediately-invoked-lambda form.

const GLOBAL_CONST = 99
make_default() = 7

a_int = (x, d=2) -> (x, d)
a_float = (x, d=1.5) -> (x, d)
a_string = (x, d="hi") -> (x, d)
a_bool = (x, d=true) -> (x, d)
a_symbol = (x, d=:sym) -> (x, d)
a_nothing = (x, d=nothing) -> (x, d)
a_missing = (x, d=missing) -> (x, d)
a_const = (x, d=GLOBAL_CONST) -> (x, d)
a_type = (x, d=Int) -> (x, d)
a_call = (x, d=make_default()) -> (x, d)
a_typed = (x, d::Union{Int,Nothing}=nothing) -> (x, d)

# Multiple positional defaults.
a_multi = (x, y=10, z=20) -> (x, y, z)

# Fully-typed parameters with a typed default.
a_typed2 = (x::Int, d::Int=7) -> (x, d)

@testset "anonymous/arrow optional default args (Issue #8047)" begin
    @test a_int(1) == (1, 2)
    @test a_int(1, 9) == (1, 9)
    @test a_float(1) == (1, 1.5)
    @test a_float(1, 9.0) == (1, 9.0)
    @test a_string(1) == (1, "hi")
    @test a_string(1, "yo") == (1, "yo")
    @test a_bool(1) == (1, true)
    @test a_bool(1, false) == (1, false)
    @test a_symbol(1) == (1, :sym)
    @test a_symbol(1, :other) == (1, :other)
    @test a_nothing(1) == (1, nothing)
    @test a_nothing(1, 5) == (1, 5)
    @test a_missing(1) === (1, missing)
    @test a_missing(1, 5) == (1, 5)
    @test a_const(1) == (1, 99)
    @test a_const(1, 5) == (1, 5)
    @test a_type(1) == (1, Int)
    @test a_type(1, Float64) == (1, Float64)
    @test a_call(1) == (1, 7)
    @test a_call(1, 5) == (1, 5)
    @test a_typed(1) == (1, nothing)
    @test a_typed(1, 5) == (1, 5)

    # Multiple defaults: all arities.
    @test a_multi(1) == (1, 10, 20)
    @test a_multi(1, 2) == (1, 2, 20)
    @test a_multi(1, 2, 3) == (1, 2, 3)

    # Typed parameters + typed default.
    @test a_typed2(4) == (4, 7)
    @test a_typed2(4, 8) == (4, 8)

    # Immediately-invoked arrow with a default.
    @test ((p, q=5) -> p + q)(3) == 8
    @test ((p, q=5) -> p + q)(3, 4) == 7
end

true
