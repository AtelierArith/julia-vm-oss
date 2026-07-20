# Issue #10268 (design/prevention) — GENERAL call-target name-resolution
# order: a function-scope PARAMETER whose name is the same as a builtin
# type-constructor name must shadow the global constructor throughout the
# function body, exactly like any other Julia local binding (upstream lexical
# scoping): local/parameter slot -> captured vars -> module const/global ->
# Base/global builtin, for EVERY surface that resolves a call target.
#
# Scope split vs. the numeric case (Issue #10146, fixed by PR #10417):
#   - PR #10417 fixed the lazy runtime specializer (src/vm/specialize/expr.rs),
#     which resolved the NUMERIC constructor names (`Float64`/`Int64`) at the
#     call site. That path is covered by
#     `functions/parameter_shadows_numeric_constructor_10146.jl` — this fixture
#     deliberately does NOT duplicate the `Float64`/`Int64` numeric spellings.
#   - This fixture covers the REMAINING surface #10417 left unfixed: static
#     return-type inference (src/compile/abstract_interp/engine/mod.rs). For a
#     non-numeric constructor name (`String`) shadowed by a parameter, the
#     return-type engine used to fall through to the builtin-constructor
#     transfer-function registry (`String` -> `Str`), mis-declaring the
#     function's return type. That was worse than a wrong value: a caller
#     compiled against the bogus `-> Str` emitted the `PrintStrNoNewline` fast
#     path and CRASHED at runtime (`Type error: expected String, got
#     "Int64"`). Reporting the safe `Any` when the callee name is bound in the
#     current function's type environment closes that gap, generalizing the
#     resolution-order invariant beyond the numeric-only names #10417 handled.
#
# Verified against upstream Julia (julia --startup-file=no): every @test below
# passes identically upstream.

# String is the consequential case: before the engine fix a caller of this
# function CRASHED ("Type error: expected String, got \"Int64\""). It runs
# through the abstract-interpretation return-type path, NOT the specializer's
# numeric arms, so it is not covered by #10417 / the numeric fixture.
string_shadow_10268(String) = String(2)

# Bool and BigInt are further non-Float64/Int64 constructor names whose
# shadowing must also resolve to the parameter, exercising the same general
# resolution-order invariant across the return-type engine.
bool_shadow_10268(Bool) = Bool(2)
bigint_shadow_10268(BigInt) = BigInt(2)

# Non-shadowed usage of a builtin-constructor name must be completely
# unaffected: `String(...)` here really constructs a String from a Char array,
# and unshadowed numeric conversion still works.
unshadowed_ctor_10268(chars) = String(chars)

# A shadowing parameter used as a plain value (never called) — a different,
# pre-existing code path — kept here so both behaviors are guarded together.
plainval_shadow_10268(BigInt) = BigInt + 1

using Test
@testset "parameter shadows non-numeric builtin type constructor (Issue #10268)" begin
    # The shadowed parameter is the callee: it is the lambda, so the body
    # calls (x -> x + 10)(2) == 12 / (x -> x * 3)(2) == 6, NOT the builtin.
    @test string_shadow_10268(x -> x + 10) == 12
    @test bool_shadow_10268(x -> x + 10) == 12
    @test bigint_shadow_10268(x -> x * 3) == 6

    # Non-shadowed builtin constructor still works normally.
    @test unshadowed_ctor_10268(['h', 'i']) == "hi"
    @test Float64(2) == 2.0
    @test Int64(2.0) == 2

    # Shadowing parameter used as a plain (non-called) value.
    @test plainval_shadow_10268(41) == 42
end

true
