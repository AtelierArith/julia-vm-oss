# A parameter type annotation naming a genuinely UNDEFINED identifier must
# raise `UndefVarError` when the method definition executes, matching upstream
# Julia's eager evaluation of signature annotations at definition time -- not
# be silently accepted as an unmatchable method (Issue #10582, the
# annotation-position sibling of Issues #10226/#10396).
#
# Root cause: signature lowering resolves an unknown annotation name to a
# nominal `JuliaType::Struct(name)` placeholder for structural dispatch, and
# nothing on the definition path ever evaluated whether the name resolves.
# The fix extends the Issue #10396 definition-time probes
# (`emit_signature_definition_probes`, `ProbeRuntimeBinding` + `Pop` per unresolvable
# bare-identifier name) to parameter annotations. Names the compiler
# statically resolves as type objects (builtins, user structs/abstract types,
# aliases) and the method's own where-binders are skipped; everything else
# must resolve at runtime or raise `UndefVarError`.
#
# Known limitations (deliberate, shared with #10396's scoping compromise):
# - compound annotations (`Vector{Undef}`, `Union{A,Undef}`, `Base.Undef`)
#   keep their permissive path;
# - a bare annotation naming a struct defined LATER in the same file is
#   statically resolvable, so it stays accepted (upstream errors) -- tracked
#   by Issue #11025;
# - KEYWORD-parameter annotations are dropped entirely at lowering
#   (`KwParam` carries no type annotation), so `f(; x::Undef = 1)` stays
#   accepted -- tracked by Issue #11024.
#
# NOTE: each failing definition sits in a PLAIN top-level `try`/`catch`
# (result discarded, flag stored via `global`): upstream Julia only evaluates
# the signature inside the `try` in that form (same caveat as the #10396
# fixture).
#
# All expectations below were verified against upstream Julia 1.12.

using Test

abstract type AnnAbs10582 end
struct AnnSub10582 <: AnnAbs10582 end
struct AnnParam10582{T} end
AnnAlias10582 = Int64
const AnnConstAlias10582 = Float64

# --- MWE reproduction: short-form definition with an undefined annotation.
mwe_err = nothing
try
    f10582(x::SomeUndefNameQQ10582) = 1
catch e
    global mwe_err = e
end

# --- After the failed definition, CALLING the function must also throw (the
# method never took effect). Only "the call throws" is asserted: the error
# class differs (upstream: UndefVarError for the unbound function name;
# sjulia: MethodError/unknown-function because the definition never
# activated).
mwe_call_threw = false
try
    f10582(1)
catch
    global mwe_call_threw = true
end

# --- Long-form `function ... end` definition.
long_err = nothing
try
    function f10582_long(x::UndefLongQQ10582)
        x
    end
catch e
    global long_err = e
end

# --- Optional (defaulted) positional parameter.
opt_err = nothing
try
    f10582_opt(x::UndefOptQQ10582 = 3) = 1
catch e
    global opt_err = e
end

# --- Vararg parameter.
vararg_err = nothing
try
    f10582_va(xs::UndefVarargQQ10582...) = 1
catch e
    global vararg_err = e
end

# --- Undefined annotation on a NON-first parameter, sibling annotations valid.
multi_err = nothing
try
    f10582_multi(x::Real, y::UndefMultiQQ10582) = x
catch e
    global multi_err = e
end

@testset "param annotation UndefVarError (Issue #10582)" begin
    @test mwe_err isa UndefVarError
    @test mwe_call_threw
    @test long_err isa UndefVarError
    @test opt_err isa UndefVarError
    @test vararg_err isa UndefVarError
    @test multi_err isa UndefVarError
end

# --- Regression guards: legitimate annotations must keep defining (and
# dispatching) with zero errors.

# Builtin types.
ok1_10582(x::Int64) = x + 1

# where-binder annotation (the binder is not a global name).
ok2_10582(x::T) where T = T

# User abstract type declared earlier at top level.
ok3_10582(x::AnnAbs10582) = "abs"

# User parametric struct, bare (UnionAll) and applied forms.
ok4_10582(x::AnnParam10582) = "bare"
ok5_10582(x::AnnParam10582{Int64}) = "applied"

# Plain global alias assignment and const alias.
ok6_10582(x::AnnAlias10582) = x + 2
ok7_10582(x::AnnConstAlias10582) = x + 0.5

# Module-qualified annotation (compound, permissive path).
ok8_10582(x::Base.Int) = x + 3

# Union annotation (compound, permissive path).
ok9_10582(x::Union{Int64, String}) = "union"

# A definition inside a constant-false branch must NOT be probed: upstream
# never evaluates the signature. Mirrors the #10396 dead-branch guard.
if false
    skipped1_10582(v::UndefNever10582A) = v
end
if false; skipped2_10582(v::UndefNever10582B) = v; end

@testset "param annotation regression guards (Issue #10582)" begin
    @test ok1_10582(1) == 2
    @test ok2_10582(1.5) === Float64
    @test ok3_10582(AnnSub10582()) == "abs"
    @test ok4_10582(AnnParam10582{String}()) == "bare"
    @test ok5_10582(AnnParam10582{Int64}()) == "applied"
    @test ok6_10582(1) == 3
    @test ok7_10582(1.0) == 1.5
    @test ok8_10582(1) == 4
    @test ok9_10582(1) == "union"
end

true
