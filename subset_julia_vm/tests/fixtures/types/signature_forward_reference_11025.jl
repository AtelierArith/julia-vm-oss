# Issue #11025: a signature annotation naming a type defined LATER in the same
# source is a FORWARD reference. Upstream Julia evaluates signature annotations
# eagerly when the method definition executes, so it raises UndefVarError at the
# (earlier) definition. sjulia used to accept it silently, because the probes
# added by #10396/#10582 skipped any name the compiler could resolve as a type
# object — and the struct table is populated for the WHOLE program regardless of
# source order.
#
# The probes now compare source-order ordinals: a type whose own definition comes
# EARLIER is skipped (its binding exists when the method definition runs), while a
# forward reference is probed and raises UndefVarError like upstream.
#
# Verified against julia 1.12.6.

using Test

# --- Types defined BEFORE the methods that annotate with them: all valid. ------
struct Early11025 end
abstract type EarlyAbstract11025 end
struct EarlyChild11025 <: EarlyAbstract11025 end

f_early_11025(x::Early11025) = 1
f_early_abstract_11025(x::EarlyAbstract11025) = 2
f_early_where_11025(x::T) where {T<:EarlyAbstract11025} = 3
f_early_builtin_11025(x::Int64) = 4
f_early_kw_11025(; x::Early11025 = Early11025()) = 5

@testset "signature annotations for earlier-defined types (Issue #11025)" begin
    @test f_early_11025(Early11025()) == 1
    @test f_early_abstract_11025(EarlyChild11025()) == 2
    @test f_early_where_11025(EarlyChild11025()) == 3
    @test f_early_builtin_11025(7) == 4
    @test f_early_kw_11025() == 5
end

# Existing Base types and earlier module-local types live in different lowered
# programs/scopes from the nested method definition. Their definition-order
# ordinals are not comparable with this source's ordinal, but both bindings are
# already visible and must not be probed as forward references (Issue #11117).
@testset "existing cross-program annotations stay visible (Issue #11117)" begin
    f_existing_rational_11117(x::Rational) = x
    f_existing_dict_11117(x::AbstractDict) = length(x)
    @test f_existing_rational_11117(1 // 2) == 1 // 2
    @test f_existing_dict_11117(Dict(:a => 1)) == 1
end

module EarlierModuleType11117
using Test

struct Local11117
    value::Int64
end

@testset "earlier module-local annotation stays visible (Issue #11117)" begin
    f_local_11117(x::Local11117) = x.value
    @test f_local_11117(Local11117(17)) == 17
end
end

# Lifted methods inside hard scopes currently have definition_order == 0. The
# byte-span fallback must still distinguish this genuine forward reference from
# the earlier Local11117 declaration above (Issue #11117).
@testset "lifted forward reference still raises (Issue #11117)" begin
    caught_forward_11117 = false
    try
        f_lifted_forward_11117(x::LaterDefined11117) = x
    catch err
        caught_forward_11117 = err isa UndefVarError
    end
    @test caught_forward_11117
end

struct LaterDefined11117 end

# --- A FORWARD reference raises UndefVarError at the definition ---------------
# `@eval`/`eval` is used so the definition executes inside the test rather than at
# top level, where it would abort the file in both runtimes.
@testset "forward-referenced annotation raises UndefVarError (Issue #11025)" begin
    # Issue #11146 (#10813 Phase 2a): these two assertions used to pass
    # VACUOUSLY. `@test_throws` ignores its expected type (Issue #10354), and
    # what sjulia actually threw here was not an `UndefVarError` — it was not an
    # exception AT ALL: `eval`'s runtime method-definition path did not implement
    # typed parameters, raised `VmError::NotImplemented`, and (by the #8664
    # mapping) bound a raw `String`, so `typeof(caught) == String`.
    #
    # sjulia now evaluates signature annotations eagerly at definition time, like
    # upstream (`vm/builtins_macro/eval.rs::probe_eval_signature_annotations`,
    # the runtime sibling of the compiled path's `Instr::ProbeRuntimeBinding`
    # probes), so a
    # forward reference raises a real `UndefVarError`.
    #
    # Asserted with an explicit `try`/`catch` + `isa` rather than `@test_throws`
    # so the TYPE is actually checked on this base (it is what the vacuous
    # `@test_throws` failed to do); the assertions are strengthened, never
    # weakened, and stay correct after #11163 fixes `@test_throws` itself.
    caught_param_11146 = nothing
    try
        eval(:(f_forward_11025(x::NotYetDefined11025) = 1))
    catch err
        caught_param_11146 = err
    end
    @test caught_param_11146 isa UndefVarError

    caught_where_11146 = nothing
    try
        eval(:(g_forward_11025(x::T) where {T<:NotYetDefined11025} = 1))
    catch err
        caught_where_11146 = err
    end
    @test caught_where_11146 isa UndefVarError
end

# NOTE: a `const` type alias used as a parameter annotation does not dispatch in
# sjulia (`const AE = E; f(x::AE)` -> MethodError). That is an independent
# pre-existing gap, tracked as Issue #11104, and is deliberately NOT exercised
# here: the probes short-circuit alias names either way, so it is orthogonal to
# the source-order fix this fixture pins.

# --- Issue #11119 regression: a MACRO-EXPANDED definition (e.g. a function
# defined inside `@testset`) carries a SYNTHETIC span with no source ordinal.
# The #11025 probes must not read that as "defined at order 0, so every type is
# a forward reference" — doing so raised a bogus UndefVarError for every struct
# annotation inside a @testset.
@testset "macro-expanded definitions keep resolving annotations (Issue #11119)" begin
    f_in_testset_11119(x::Early11025) = 11
    @test f_in_testset_11119(Early11025()) == 11

    g_in_testset_11119(x::T) where {T<:EarlyAbstract11025} = 12
    @test g_in_testset_11119(EarlyChild11025()) == 12

    h_in_testset_11119(; x::Early11025 = Early11025()) = 13
    @test h_in_testset_11119() == 13
end

true
