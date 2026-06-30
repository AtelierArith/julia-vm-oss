using Test

# Systematic (definition-form × default-value node-kind) coverage for
# default-argument extraction and stub generation.
#
# Prevention follow-up to Issue #8017 (#8040). The original #8017 bug was a
# heuristic in `extract_default_from_parameter_node`
# (subset_julia_vm/src/lowering/function/defaults.rs) that skipped every bare
# `Identifier` child as if it were a type annotation, dropping bare-identifier
# defaults so no reduced-arity stub was generated → `No method matching …`.
# Existing fixtures only exercised the SHORT form with LITERAL defaults, so the
# (form × value-kind) matrix where the bug actually lived was never tested.
# #8039 added a single block-form fixture; this generalizes it into a guard for
# the whole class.
#
# Two axes are crossed:
#   Axis 1 (definition form): short form `f(...) = e`, block form
#       `function f(...) ... end`.  (The anonymous/arrow form `(x, d=2) -> e` is
#       now supported too — Issue #8047 — and is exercised in its own fixture
#       `anonymous_default_arg_8047.jl`, so it is not duplicated here.)
#   Axis 2 (default-value node kind): integer / float / string / bool / symbol
#       literal, bare identifier (nothing / missing / global const), type name
#       (Int), call (`f()`), and typed-param-with-default (`x::T = nothing`).
#
# For every (form × node-kind) cell we assert BOTH the reduced-arity call
# (default applied) and the full-arity call (default overridden) dispatch and
# return the upstream-julia value.

const GLOBAL_CONST = 99
make_default() = 7

# ── Axis 1: SHORT FORM  f(...) = e ──────────────────────────────────────────
s_int(x, d=2) = (x, d)
s_float(x, d=1.5) = (x, d)
s_string(x, d="hi") = (x, d)
s_bool(x, d=true) = (x, d)
s_symbol(x, d=:sym) = (x, d)
s_nothing(x, d=nothing) = (x, d)
s_missing(x, d=missing) = (x, d)
s_const(x, d=GLOBAL_CONST) = (x, d)
s_type(x, d=Int) = (x, d)
s_call(x, d=make_default()) = (x, d)
s_typed(x, d::Union{Int,Nothing}=nothing) = (x, d)

# ── Axis 1: BLOCK FORM  function f(...) ... end ─────────────────────────────
function b_int(x, d=2)
    return (x, d)
end
function b_float(x, d=1.5)
    return (x, d)
end
function b_string(x, d="hi")
    return (x, d)
end
function b_bool(x, d=true)
    return (x, d)
end
function b_symbol(x, d=:sym)
    return (x, d)
end
function b_nothing(x, d=nothing)
    return (x, d)
end
function b_missing(x, d=missing)
    return (x, d)
end
function b_const(x, d=GLOBAL_CONST)
    return (x, d)
end
function b_type(x, d=Int)
    return (x, d)
end
function b_call(x, d=make_default())
    return (x, d)
end
function b_typed(x, d::Union{Int,Nothing}=nothing)
    return (x, d)
end

@testset "default-arg (form × node-kind) matrix (Issue #8040)" begin
    # ── short form  f(...) = e ──────────────────────────────────────────────
    @test s_int(1) == (1, 2)
    @test s_int(1, 9) == (1, 9)
    @test s_float(1) == (1, 1.5)
    @test s_float(1, 9.0) == (1, 9.0)
    @test s_string(1) == (1, "hi")
    @test s_string(1, "yo") == (1, "yo")
    @test s_bool(1) == (1, true)
    @test s_bool(1, false) == (1, false)
    @test s_symbol(1) == (1, :sym)
    @test s_symbol(1, :other) == (1, :other)
    @test s_nothing(1) == (1, nothing)
    @test s_nothing(1, 5) == (1, 5)
    @test s_missing(1) === (1, missing)
    @test s_missing(1, 5) == (1, 5)
    @test s_const(1) == (1, 99)
    @test s_const(1, 5) == (1, 5)
    @test s_type(1) == (1, Int)
    @test s_type(1, Float64) == (1, Float64)
    @test s_call(1) == (1, 7)
    @test s_call(1, 5) == (1, 5)
    @test s_typed(1) == (1, nothing)
    @test s_typed(1, 5) == (1, 5)

    # ── block form  function f(...) ... end ─────────────────────────────────
    @test b_int(1) == (1, 2)
    @test b_int(1, 9) == (1, 9)
    @test b_float(1) == (1, 1.5)
    @test b_float(1, 9.0) == (1, 9.0)
    @test b_string(1) == (1, "hi")
    @test b_string(1, "yo") == (1, "yo")
    @test b_bool(1) == (1, true)
    @test b_bool(1, false) == (1, false)
    @test b_symbol(1) == (1, :sym)
    @test b_symbol(1, :other) == (1, :other)
    @test b_nothing(1) == (1, nothing)
    @test b_nothing(1, 5) == (1, 5)
    @test b_missing(1) === (1, missing)
    @test b_missing(1, 5) == (1, 5)
    @test b_const(1) == (1, 99)
    @test b_const(1, 5) == (1, 5)
    @test b_type(1) == (1, Int)
    @test b_type(1, Float64) == (1, Float64)
    @test b_call(1) == (1, 7)
    @test b_call(1, 5) == (1, 5)
    @test b_typed(1) == (1, nothing)
    @test b_typed(1, 5) == (1, 5)
end

true
