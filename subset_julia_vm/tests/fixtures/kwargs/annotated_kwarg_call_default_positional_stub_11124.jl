# Issue #11124: a type-annotated keyword whose default is a CALL lost its default
# (bound `#undef`) whenever the function ALSO had a defaulted positional argument.
#
# `Value::Undef` in a keyword slot is the VM's NOT-SUPPLIED sentinel — Julia
# surface syntax cannot pass `#undef`. It marks a required keyword and, for a
# body-evaluated default (Issue #5121), tells the callee's prologue to
# materialize the real default (`k === Undef ? k = <default expr> : k`).
#
# A defaulted positional argument lowers to a reduced-arity forwarding stub
# (`g(y, x=2; ...)` => `g(y) = g(y, 2)`). That stub has NO prologue: it forwards
# its own raw `k` slot verbatim through `CallWithKwargs`, so the sentinel reaches
# the callee as an EXPLICITLY PRESENT keyword. The Issue #11024 assertion then
# asserted the sentinel and raised
# `TypeError: in keyword argument k, expected Integer, got a value of type #undef`
# for a keyword the caller never supplied.
#
# All three of (defaulted positional) x (type-annotated kwarg) x (kwarg default
# is a CALL) are required to trigger it; the assertion now skips the sentinel.
#
# This regressed main via PR #11082 (which landed #11024) and turned
# packages::chunk_003 (packages_quadgk_inplace_batch_8289) red — QuadGK's
# `BatchIntegrand(f!, y, x=similar(y, Nothing); max_batch::Integer=typemax(Int))`
# is exactly this shape.
#
# Verified against julia 1.12.6.

using Test

# --- The 3-way trigger matrix ------------------------------------------------
# (a) THE BUG: pos-default x annotated kwarg x CALL default
a11124(y, x=2; k::Integer=typemax(Int)) = (y, x, k)
# (b) control: pos-default x UNANNOTATED kwarg x call default
b11124(y, x=2; k=typemax(Int)) = (y, x, k)
# (c) control: pos-default x annotated kwarg x LITERAL default
c11124(y, x=2; k::Integer=99) = (y, x, k)
# (d) control: NO pos-default x annotated kwarg x call default
d11124(y; k::Integer=typemax(Int)) = (y, k)

@testset "annotated kwarg with a call default behind a positional-default stub (Issue #11124)" begin
    # The failing shape: the call default must be materialized, not left #undef.
    @test a11124(1) == (1, 2, typemax(Int))
    # Supplying the positional still defaults the keyword.
    @test a11124(1, 5) == (1, 5, typemax(Int))
    # Supplying the keyword overrides the default.
    @test a11124(1; k=7) == (1, 2, 7)

    # The three controls (each already passed before the fix) stay correct.
    @test b11124(1) == (1, 2, typemax(Int))
    @test c11124(1) == (1, 2, 99)
    @test d11124(1) == (1, typemax(Int))
end

# --- The sentinel skip must NOT swallow the required-keyword check ------------
# `Undef` is ALSO the required-kwarg marker, and the stub forwards it verbatim,
# so an omitted required keyword must still raise UndefKeywordError (not bind
# the sentinel) — through the stub and without it.
req_stub_11124(y, x=2; k::Integer) = (y, x, k)
req_plain_11124(; k::Integer) = k

@testset "omitted required annotated keyword still raises UndefKeywordError (Issue #11124)" begin
    @test_throws UndefKeywordError req_stub_11124(1)
    @test_throws UndefKeywordError req_plain_11124()
    # ... and supplying it works.
    @test req_stub_11124(1; k=5) == (1, 2, 5)
    @test req_plain_11124(k=5) == 5
end

# --- The Issue #11024 assertion must STILL fire for a SUPPLIED value ----------
# Skipping the sentinel must not weaken the declared-type assertion on values the
# caller actually supplies — including through the positional-default stub.
concrete_11124(; x::Int64=1) = x
abstract_11124(; x::Real=1) = x
stub_supplied_11124(y, x=2; k::Integer=typemax(Int)) = (y, x, k)

@testset "declared keyword type still asserted on supplied values (Issues #11024, #11124)" begin
    @test_throws TypeError concrete_11124(x=2.0)
    @test_throws TypeError stub_supplied_11124(1; k=2.5)
    # An abstract annotation stays permissive (upstream asserts, never converts).
    @test abstract_11124(x=2.5) == 2.5
    @test concrete_11124(x=3) == 3
    @test stub_supplied_11124(1; k=7) == (1, 2, 7)
end

true
