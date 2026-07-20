# Issue #10407 — a `where`-clause binder whose name collides with a
# builtin/global type name (`h(x::Float64) where {Float64}`) must be treated
# as a fresh, method-local TypeVar shadowing the global BOTH in the signature
# and in the body, exactly as upstream Julia scopes it:
#
#   1. Signature/dispatch: `x::Float64` under `where {Float64}` behaves like
#      `x::T` under `where {T}` — any argument type is dispatch-eligible
#      (subject to the binder's declared bounds), NOT only the literal
#      builtin `Float64`. Before the fix sjulia raised a spurious
#      `MethodError: no method matching h(::Int64)` on `h(3)`.
#   2. Body binding: the per-call TypeVar-to-local installation must not be
#      bypassed just because the name also resolves to a builtin — the body's
#      `Float64(2)` calls/converts through the TypeVar's actual bound type
#      (`Int64` for `h(3)`), not the builtin `Float64` constructor. Before
#      the fix the lazy runtime specializer baked the builtin conversion into
#      the specialized body, so `h(3)` returned `2.0::Float64`.
#
# Explicitly distinct from the call-site name-resolution fix for a PARAMETER
# shadowing a builtin constructor (Issues #10146/#10268, PR #10417) — this is
# a method-dispatch/type-parameter-binding gap.
#
# Verified against upstream Julia 1.12 (julia --startup-file=no): every @test
# below passes identically upstream (upstream additionally prints a "declares
# type variable ... but does not use it" WARNING for the unused-binder case,
# which does not affect the results).

# MWE 1/2 from the Issue: short-form definition, binder name `Float64`.
h_shadow_10407(x::Float64) where {Float64} = Float64(2)

# Same shadowing through the full `function ... end` form.
function hf_shadow_10407(x::Float64) where {Float64}
    Float64(2)
end

# The binder loaded as a plain value in the body: must be the per-call bound
# type, not the builtin type object.
ht_shadow_10407(x::Float64) where {Float64} = Float64

# Bounded colliding binder: the bound restricts dispatch like any TypeVar.
hb_shadow_10407(x::Float64) where {Float64<:Integer} = Float64(2)

# A DIFFERENT name that merely aliases the same type is NOT shadowed: the
# binder shadows the NAME `Int64` as written, so `x::Int` still resolves to
# the global `Int` alias (= the builtin Int64) and restricts dispatch.
hint_shadow_10407(x::Int) where {Int64} = 1

# Unshadowed control cases.
f_control_10407(x::T) where {T} = T(2)
plain_ctor_10407(x) = Float64(x)

using Test
@testset "where binder shadows builtin type name (Issue #10407)" begin
    # (1) dispatch accepts any argument type; (2) body call goes through the
    # per-call TypeVar binding.
    @test h_shadow_10407(3) === 2
    @test h_shadow_10407(3.5) === 2.0
    @test typeof(h_shadow_10407(3)) === Int64
    @test typeof(h_shadow_10407(3.5)) === Float64

    # Full-form definition behaves identically.
    @test hf_shadow_10407(3) === 2
    @test typeof(hf_shadow_10407(3)) === Int64

    # Binder as a plain value load resolves to the bound type per call.
    @test ht_shadow_10407(3) === Int64
    @test ht_shadow_10407(3.5) === Float64

    # Declared bounds still apply to the colliding binder.
    @test hb_shadow_10407(3) === 2
    @test typeof(hb_shadow_10407(3)) === Int64
    @test_throws MethodError hb_shadow_10407(3.5)

    # `x::Int` under `where {Int64}` is NOT rebound (name-based shadowing).
    @test hint_shadow_10407(3) === 1
    @test_throws MethodError hint_shadow_10407(3.5)

    # Non-colliding where binder keeps working (control case).
    @test f_control_10407(3) === 2
    @test f_control_10407(3.5) === 2.0

    # Unshadowed builtin constructor/conversion is unaffected.
    @test plain_ctor_10407(3) === 3.0
    @test Float64(2) === 2.0
    @test Int64(2.0) === 2
end

true
