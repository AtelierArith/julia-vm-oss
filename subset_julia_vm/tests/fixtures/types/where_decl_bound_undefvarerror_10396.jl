# A DECLARATION-position (function-signature) `where`-bound naming an
# undefined identifier must raise `UndefVarError` when the method definition
# executes, matching upstream Julia's eager evaluation of signature bounds at
# definition time -- not be silently accepted (Issue #10396, the
# declaration-position sibling of Issue #10226's value-position fix).
#
# Root cause: function-signature `where` clauses construct
# `TypeParam{name, upper_bound, lower_bound}` records
# (subset_julia_vm_lowering/src/lowering/function/where_clause.rs) whose bounds were
# treated purely as string constraints for structural dispatch matching --
# nothing on the declaration path ever evaluated whether the bound name
# resolves. The fix: the compiler now emits a definition-time resolution
# probe (`LoadAny` + `Pop`) per unresolvable bare-identifier bound name right
# before the definition activates. Names the compiler statically resolves as
# type objects (builtins, user structs/abstract types, aliases) and the
# method's own where-binders are skipped; everything else must resolve at
# runtime (enclosing where type-bindings, then globals) or raise
# `UndefVarError` -- exactly the resolution semantics #10226 established for
# value-position bounds.
#
# NOTE: each failing definition sits in a PLAIN top-level `try`/`catch`
# (result discarded, flag stored via `global`): upstream Julia only evaluates
# the signature inside the `try` in that form -- wrapping the `try` in an
# assignment (`err = try ... end`) hoists the method definition out of the
# `try` and the UndefVarError escapes as a top-level LoadError.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

abstract type DeclAbs10396 end
struct DeclSub10396 <: DeclAbs10396 end
struct DeclParam10396{T} end
DeclAlias10396 = Int64

# --- MWE reproduction: short-form definition with an undefined upper bound.
# Upstream raises UndefVarError at the definition itself (before any call).
mwe_err = nothing
try
    h10396(x::T) where T<:UndefZZZ10396 = x
catch e
    global mwe_err = e
end

# --- Misleading-downstream-error guard: after the failed definition above,
# CALLING the function must also throw (the method never took effect). The
# concrete error class differs across interpreters (upstream: UndefVarError
# for the unbound function name; sjulia: MethodError from the world-age gate
# because the definition never activated), so only "the call throws" is
# asserted -- the important property is that the call cannot silently reach a
# dispatched method body.
mwe_call_threw = false
try
    h10396(1)
catch
    global mwe_call_threw = true
end

# --- Lower bound (`>:`) with an undefined name.
lower_err = nothing
try
    g10396(x::T) where T>:UndefLower10396 = x
catch e
    global lower_err = e
end

# --- Compound (two-sided) bound: undefined LOWER bound is reported first,
# mirroring upstream's left-to-right TypeVar(:T, lb, ub) evaluation.
compound_lower_err = nothing
try
    c10396(x::T) where UndefLo10396<:T<:Real = x
catch e
    global compound_lower_err = e
end

# --- Compound (two-sided) bound: undefined UPPER bound.
compound_upper_err = nothing
try
    d10396(x::T) where Int64<:T<:UndefHi10396 = x
catch e
    global compound_upper_err = e
end

# --- Long-form `function ... end` definition with an undefined bound.
long_err = nothing
try
    function h10396_long(x::T) where T<:UndefQQ10396
        x
    end
catch e
    global long_err = e
end

# --- Multi-parameter braced clause: the undefined bound is caught even when
# sibling parameters are legitimately bounded.
braced_err = nothing
try
    b10396(x::T, y::S) where {T<:Real, S<:UndefWW10396} = x
catch e
    global braced_err = e
end

@testset "declaration-position where-bound UndefVarError (Issue #10396)" begin
    @test mwe_err isa UndefVarError
    @test mwe_call_threw
    @test lower_err isa UndefVarError
    @test compound_lower_err isa UndefVarError
    @test compound_upper_err isa UndefVarError
    @test long_err isa UndefVarError
    @test braced_err isa UndefVarError
end

# --- Regression guards: legitimate declaration-position `where` usage must
# keep defining (and dispatching) with zero errors. Definitions live at top
# level, matching the fixture convention for method definitions.

# Builtin bound -- the overwhelmingly common case.
ok1_10396(x::T) where T<:Real = x

# Compound two-sided bound with both sides defined.
ok2_10396(x::T) where Int64<:T<:Real = x

# Sibling binder reference inside one braced clause.
ok3_10396(x::T, y::S) where {T, S<:T} = (x, y)

# Chained clause: inner bound references the outer binder.
ok4_10396(x::S) where S<:T where T = x

# Bound naming a user abstract type declared earlier at top level.
ok5_10396(x::T) where T<:DeclAbs10396 = x

# Bound naming a user parametric struct (UnionAll base).
ok6_10396(x::S) where S<:DeclParam10396 = x

# Bound through a plain global alias assignment.
ok7_10396(x::T) where T<:DeclAlias10396 = x

# A definition inside a constant-false branch must NOT be probed: upstream
# never evaluates the signature (the branch body does not execute), so the
# undefined bound name must not raise. Guards the adversarial-verification
# finding on PR #10594 (probe hoisted out of an eliminated branch).
if false
    skipped1_10396(v::T) where T<:UndefNever10396A = v
end
if false; skipped2_10396(v::T) where T<:UndefNever10396B = v; end
cond_false_10396 = false
if cond_false_10396
    skipped3_10396(v::T) where T<:UndefNever10396C = v
end
ok8_10396 = 8

@testset "declaration-position where-bound regression guards (Issue #10396)" begin
    @test ok1_10396(1) == 1
    @test ok2_10396(2) == 2
    @test ok3_10396(1, 2) == (1, 2)
    @test ok4_10396(3) == 3
    @test ok5_10396(DeclSub10396()) == DeclSub10396()
    @test ok6_10396(DeclParam10396{Int64}()) isa DeclParam10396
    @test ok7_10396(4) == 4
    @test ok8_10396 == 8
end

true
