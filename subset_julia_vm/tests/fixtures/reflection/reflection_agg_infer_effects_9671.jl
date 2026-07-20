# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: reflection/infer_effects_builtin_categories_4274.jl =====

# Issue #4274: Core builtin effect/exception metadata is composed from upstream
# semantic category sets (`_PURE_BUILTINS`, `_CONSISTENT_BUILTINS`,
# `_EFFECT_FREE_BUILTINS`, `_INACCESSIBLEMEM_BUILTINS`) plus a per-call `nothrow`
# decision, mirroring `julia/Compiler/src/tfuncs.jl builtin_effects` /
# `builtin_exct`, instead of an accidental proven-total fallback.
#
# Every expected value was captured field-for-field from upstream Julia 1.12 via
#   Base.infer_effects(f, sig)        # effect show string
#   Base.infer_exception_type(f, sig) # inferred exception type
# and must match exactly.

@testset "infer_effects pure Core builtins (#4274)" begin
    # Pure builtins (`tuple`, `typeof`, `nfields`) are consistent, effect-free,
    # nothrow, and touch no externally accessible mutable memory: TOTAL.
    @test string(Base.infer_effects(tuple, Tuple{Int64,Float64})) ==
        "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(tuple, Tuple{Int64,Float64}) === Union{}

    @test string(Base.infer_effects(typeof, Tuple{Int64})) ==
        "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(typeof, Tuple{Int64}) === Union{}

    @test string(Base.infer_effects(nfields, Tuple{Int64})) ==
        "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(nfields, Tuple{Int64}) === Union{}
end

@testset "infer_effects consistent/effect-free nothrow builtins (#4274)" begin
    # `isa`, `typeassert`, `sizeof`, `ifelse` are consistent + effect-free and,
    # for these well-typed concrete signatures, nothrow: TOTAL.
    @test string(Base.infer_effects(isa, Tuple{Int64,Type{Int64}})) ==
        "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(isa, Tuple{Int64,Type{Int64}}) === Union{}

    @test string(Base.infer_effects(typeassert, Tuple{Int64,Type{Int64}})) ==
        "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(typeassert, Tuple{Int64,Type{Int64}}) === Union{}

    @test string(Base.infer_effects(sizeof, Tuple{Int64})) ==
        "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(sizeof, Tuple{Int64}) === Union{}

    @test string(Base.infer_effects(ifelse, Tuple{Bool,Int64,Int64})) ==
        "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(ifelse, Tuple{Bool,Int64,Int64}) === Union{}
end

@testset "infer_effects throwing Core builtin taints nothrow (#4274)" begin
    # `fieldtype` is consistent + effect-free + inaccessiblememonly but may throw
    # (e.g. an out-of-range field index), so it taints `nothrow` and surfaces an
    # `Any` inferred exception type, exactly like upstream `builtin_exct`.
    @test string(Base.infer_effects(fieldtype, Tuple{Type{Complex{Int64}},Int64})) ==
        "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(fieldtype, Tuple{Type{Complex{Int64}},Int64}) === Any
end

# ===== source: reflection/infer_effects_exception_classification_4274.jl =====

# Per-signature effect classification for Base.infer_effects /
# Base.infer_exception_type covering array helpers (Issue #4972), method/type
# helpers (Issue #4957), and type-callable constructors (Issue #4991, #4274).
#
# All expected values were captured from upstream Julia 1.12 with
#   Base.infer_effects(f, sig) ; Base.infer_exception_type(f, sig)
# and must match field-for-field (effect show string + exception type).

@testset "infer_effects array helper classification (#4972)" begin
    # fill / zeros allocate: not consistent, not nothrow, may not terminate.
    @test string(Base.infer_effects(fill, Tuple{Int64,Tuple{Int64,Int64}})) ==
        "(!c,+e,!n,!t,+s,+m,!u,+o,+r)"
    @test Base.infer_exception_type(fill, Tuple{Int64,Tuple{Int64,Int64}}) === Any
    @test string(Base.infer_effects(zeros, Tuple{Type{Float64},Int64})) ==
        "(!c,+e,!n,!t,+s,+m,!u,+o,+r)"
    @test Base.infer_exception_type(zeros, Tuple{Type{Float64},Int64}) === Any

    # reshape / vec can throw DimensionMismatch (reshape also ArgumentError).
    @test string(Base.infer_effects(reshape, Tuple{Vector{Int64},Int64,Int64})) ==
        "(?c,+e,!n,+t,+s,?m,+u,+o,+r)"
    @test Base.infer_exception_type(reshape, Tuple{Vector{Int64},Int64,Int64}) ===
        Union{DimensionMismatch,ArgumentError}
    @test string(Base.infer_effects(vec, Tuple{Matrix{Int64}})) ==
        "(?c,+e,!n,+t,+s,?m,+u,+o,+r)"
    @test Base.infer_exception_type(vec, Tuple{Matrix{Int64}}) === DimensionMismatch

    # fill! mutates in place; throws BoundsError.
    @test string(Base.infer_effects(fill!, Tuple{Vector{Int64},Int64})) ==
        "(!c,?e,!n,!t,+s,?m,!u,+o,+r)"
    @test Base.infer_exception_type(fill!, Tuple{Vector{Int64},Int64}) === BoundsError

    # insert! / splice! mutate; effects differ.
    @test string(Base.infer_effects(insert!, Tuple{Vector{Int64},Int64,Int64})) ==
        "(!c,!e,!n,+t,+s,!m,+u,+o,!r)"
    @test Base.infer_exception_type(insert!, Tuple{Vector{Int64},Int64,Int64}) === Any
    @test string(Base.infer_effects(splice!, Tuple{Vector{Int64},Int64})) ==
        "(!c,!e,!n,!t,!s,!m,!u,!o,!r)"
    @test Base.infer_exception_type(splice!, Tuple{Vector{Int64},Int64}) === Any
end

@testset "infer_effects method/type helper classification (#4957)" begin
    @test string(Base.infer_effects(applicable, Tuple{typeof(+),Int64,Int64})) ==
        "(!c,!e,!n,+t,+s,!m,+u,+o,+r)"
    @test Base.infer_exception_type(applicable, Tuple{typeof(+),Int64,Int64}) === Any

    @test string(Base.infer_effects(which, Tuple{typeof(+),Type{Tuple{Int64,Int64}}})) ==
        "(!c,!e,!n,!t,!s,!m,!u,!o,!r)"
    @test Base.infer_exception_type(which, Tuple{typeof(+),Type{Tuple{Int64,Int64}}}) === Any

    @test string(Base.infer_effects(methods, Tuple{typeof(+)})) ==
        "(!c,!e,!n,!t,!s,!m,!u,!o,!r)"
    @test Base.infer_exception_type(methods, Tuple{typeof(+)}) === Any

    # fieldoffset: index form has no inferred exception; the Symbol form has no
    # matching method, so the inferred exception type is MethodError.
    @test string(Base.infer_effects(fieldoffset, Tuple{DataType,Int64})) ==
        "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(fieldoffset, Tuple{DataType,Int64}) === Any
    @test string(Base.infer_effects(fieldoffset, Tuple{DataType,Symbol})) ==
        "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(fieldoffset, Tuple{DataType,Symbol}) === MethodError

    # typejoin / typeintersect are total and cannot throw.
    @test string(Base.infer_effects(typejoin, Tuple{Type{Int64},Type{Float64}})) ==
        "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(typejoin, Tuple{Type{Int64},Type{Float64}}) === Union{}
    @test string(Base.infer_effects(typeintersect, Tuple{Type{Int64},Type{Real}})) ==
        "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(typeintersect, Tuple{Type{Int64},Type{Real}}) === Union{}
end

@testset "infer_effects type-callable constructor classification (#4991)" begin
    # Int64(::Float64) / Bool(::Int64) can throw InexactError but are otherwise
    # total. Must not hang while resolving the DataType callable name.
    @test string(Base.infer_effects(Int64, Tuple{Float64})) ==
        "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(Int64, Tuple{Float64}) === InexactError
    @test string(Base.infer_effects(Bool, Tuple{Int64})) ==
        "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(Bool, Tuple{Int64}) === InexactError

    # Float64(::Int64) is total and cannot throw.
    @test string(Base.infer_effects(Float64, Tuple{Int64})) ==
        "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(Float64, Tuple{Int64}) === Union{}
end

# ===== source: reflection/infer_effects_intdiv_4274.jl =====

# Issue #4274: integer division / remainder helpers (`div`, `rem`, `mod`, `fld`,
# `cld`) over integer arguments throw `DivideError` (division by zero, or
# `typemin ÷ -1` overflow). Upstream `Base.infer_exception_type` reports
# `DivideError` and `Base.infer_effects` reports the total-except-`nothrow`
# record `(+c,+e,!n,+t,+s,+m,+u,+o,+r)` for these signatures. Before this slice
# sjulia collapsed them to the proven-total fallback (`Union{}` / all-true).
#
# The classification is keyed by name + all-`Integer` argument types so the
# float overloads (which never throw `DivideError`) and mixed int/float
# overloads keep falling through to the proven-total representative, exactly as
# upstream. `Bool <: Integer`, so `div(Bool, Bool)` is covered too. Values
# verified field-for-field against upstream Julia 1.12.6.

@testset "reflection integer div family exception type (#4274)" begin
    @test Base.infer_exception_type(div, Tuple{Int64,Int64}) === DivideError
    @test Base.infer_exception_type(rem, Tuple{Int64,Int64}) === DivideError
    @test Base.infer_exception_type(mod, Tuple{Int64,Int64}) === DivideError
    @test Base.infer_exception_type(fld, Tuple{Int64,Int64}) === DivideError
    @test Base.infer_exception_type(cld, Tuple{Int64,Int64}) === DivideError

    # Mixed integer widths and Bool are still all-`Integer`, so still DivideError.
    @test Base.infer_exception_type(div, Tuple{Int32,Int32}) === DivideError
    @test Base.infer_exception_type(mod, Tuple{Int64,Int32}) === DivideError
    @test Base.infer_exception_type(rem, Tuple{Int8,Int8}) === DivideError
    @test Base.infer_exception_type(div, Tuple{Bool,Bool}) === DivideError
end

@testset "reflection integer div family effects nothrow (#4274)" begin
    for f in (div, rem, mod, fld, cld)
        e = Base.infer_effects(f, Tuple{Int64,Int64})
        @test e.nothrow === false
        @test string(e) == "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    end
end

@testset "reflection div family float overloads stay total (#4274)" begin
    # Float division never throws DivideError, so these keep the proven-total
    # representative with no inferred exception.
    for f in (div, rem, mod, fld, cld)
        @test Base.infer_exception_type(f, Tuple{Float64,Float64}) === Union{}
        @test string(Base.infer_effects(f, Tuple{Float64,Float64})) ==
            "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    end
    # Mixed int/float is not all-`Integer`, so it also falls through to total.
    @test Base.infer_exception_type(mod, Tuple{Int64,Float64}) === Union{}
    @test string(Base.infer_effects(mod, Tuple{Int64,Float64})) ==
        "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
end

# ===== source: reflection/infer_effects_math_4970.jl =====

# Issue #4970: throwing math helpers must report their documented inferred
# exception type and a non-`nothrow` effect, instead of the blanket proven-total
# (Union{} / all-true) fallback. Values verified against upstream Julia 1.12.
#
# Issue #4986: Method reflection retains the `purity` field populated from
# `@assume_effects` metadata; the dedicated probe locks parity (already resolved
# upstream by commit ca49737c, fixture added here to guard against regression).

@testset "reflection infer_exception_type math helpers (#4970)" begin
    @test Base.infer_exception_type(sin, Tuple{Float64}) === DomainError
    # cos / sqrt over Float64 also throw DomainError (extended #4970).
    @test Base.infer_exception_type(cos, Tuple{Float64}) === DomainError
    @test Base.infer_exception_type(sqrt, Tuple{Float64}) === DomainError
    @test Base.infer_exception_type(log1p, Tuple{Float64}) === Union{DomainError,InexactError}
    # log over Float64 has the same Union as log1p (extended #4970).
    @test Base.infer_exception_type(log, Tuple{Float64}) === Union{DomainError,InexactError}
    @test Base.infer_exception_type(divrem, Tuple{Int64,Int64}) === DivideError
    @test Base.infer_exception_type(gcd, Tuple{Int64,Int64}) === OverflowError
    @test Base.infer_exception_type(lcm, Tuple{Int64,Int64}) === Union{DivideError,OverflowError}

    # exp is proven total -> no exception.
    @test Base.infer_exception_type(exp, Tuple{Float64}) === Union{}
end

@testset "reflection infer_effects math helpers nothrow (#4970)" begin
    # Throwing math helpers are total except for `nothrow`, which is false.
    for f in (sin, cos, sqrt, log, log1p)
        e = Base.infer_effects(f, Tuple{Float64})
        @test e.nothrow === false
        @test string(e) == "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    end
    for f in (divrem, gcd, lcm)
        e = Base.infer_effects(f, Tuple{Int64,Int64})
        @test e.nothrow === false
        @test string(e) == "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    end

    # exp stays proven-total.
    ee = Base.infer_effects(exp, Tuple{Float64})
    @test ee.nothrow === true
    @test string(ee) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
end

@testset "reflection Method.purity retained (#4986)" begin
    reflection_plain_4986(x) = x + 1
    m = first(methods(reflection_plain_4986))
    @test m.purity === UInt16(0)
    @test typeof(m.purity) === UInt16
end

# ===== source: reflection/infer_effects_tuple_range_helpers_4974.jl =====

# Issue #4974: representative tuple / pair / range helper signatures must report
# the upstream `Base.infer_effects` / `Base.infer_exception_type` records.
#
# Tuple `first`/`last`/`length`/`isempty`/`reverse`/`only` are total (no
# exception). `getindex(::Pair, ::Int)` is total-except-nothrow (may throw an
# out-of-range index). `eachindex(::AbstractVector)` is the refined
# `(?c,+e,+n,+t,+s,?m,+u,+o,+r)` (consistent-if-inaccessiblememonly /
# inaccessiblemem-or-argmemonly) and is nothrow. `collect(::AbstractRange)`
# allocates: `(!c,+e,!n,!t,+s,+m,!u,+o,+r)`, exception `Any` — upstream infers
# the same record for every concrete range type (`UnitRange`, `Base.OneTo`,
# `StepRange`), so `UnitRange{Int64}` is used here for a spelling that resolves
# identically under both `sjulia` and upstream `julia`. `tuple(...)` is a Core
# builtin already classified by the #4274 builtin layer (total). Values captured
# field-for-field from Julia 1.12.6.

@testset "infer_effects tuple helpers total (#4974)" begin
    for f in (first, last, length, isempty, reverse)
        @test string(Base.infer_effects(f, Tuple{Tuple{Int64,Int64}})) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
        @test Base.infer_exception_type(f, Tuple{Tuple{Int64,Int64}}) === Union{}
    end
    @test string(Base.infer_effects(only, Tuple{Tuple{Int64}})) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(only, Tuple{Tuple{Int64}}) === Union{}

    # tuple builtin (classified by the #4274 builtin-category layer) is total.
    @test string(Base.infer_effects(tuple, Tuple{Int64,Float64})) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(tuple, Tuple{Int64,Float64}) === Union{}
end

@testset "infer_effects getindex(::Pair, ::Int) total-except-nothrow (#4974)" begin
    @test string(Base.infer_effects(getindex, Tuple{Pair{Int64,Int64},Int64})) == "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(getindex, Tuple{Pair{Int64,Int64},Int64}) === Any
end

@testset "infer_effects eachindex / collect range helpers (#4974)" begin
    @test string(Base.infer_effects(eachindex, Tuple{Vector{Int64}})) == "(?c,+e,+n,+t,+s,?m,+u,+o,+r)"
    @test Base.infer_exception_type(eachindex, Tuple{Vector{Int64}}) === Union{}

    @test string(Base.infer_effects(collect, Tuple{UnitRange{Int64}})) == "(!c,+e,!n,!t,+s,+m,!u,+o,+r)"
    @test Base.infer_exception_type(collect, Tuple{UnitRange{Int64}}) === Any
end

@testset "infer_effects tuple/pair helpers do not intercept other overloads (#4974)" begin
    # getindex(::Pair, ::Int) classification must not affect getindex(::Vector,…).
    @test string(Base.infer_effects(getindex, Tuple{Vector{Int64},Int64})) != "(+c,+e,!n,+t,+s,+m,+u,+o,+r)" ||
          Base.infer_exception_type(getindex, Tuple{Vector{Int64},Int64}) === Union{}
    # collect(::AbstractRange) must not intercept collect(::Vector).
    @test string(Base.infer_effects(collect, Tuple{Vector{Int64}})) != "(!c,+e,!n,!t,+s,+m,!u,+o,+r)"
end

# ===== source: reflection/infer_effects_type_callable_4987.jl =====

# Issue #4987: the VM-backed reflection helper rejected `DataType` callables
# (constructors such as `Int64`, `Bool`, `Float64`) with
# "Expected function, string, or symbol" before the pure-Julia effect
# classifier could run. `extract_func_name` now keys a `DataType` callable by
# its type name, mirroring `nameof`, so `methods` / `which` / `hasmethod` and
# the `infer_effects` / `infer_exception_type` surface all accept type
# callables. Expected values captured from upstream Julia 1.12.

@testset "infer_effects two-arg type callable (#4987)" begin
    # Int64 / Bool conversions from floating inputs can throw InexactError.
    @test Base.infer_exception_type(Int64, Tuple{Float64}) === InexactError
    @test Base.infer_effects(Int64, Tuple{Float64}).nothrow == false
    @test Base.infer_exception_type(Bool, Tuple{Float64}) === InexactError
    @test Base.infer_effects(Bool, Tuple{Float64}).nothrow == false

    # Float64 from an integer input is total and cannot throw.
    @test Base.infer_effects(Float64, Tuple{Int64}).nothrow == true
    @test Base.infer_exception_type(Float64, Tuple{Int64}) === Union{}
end

@testset "reflection helpers accept type callables (#4987)" begin
    # Previously these raised "Type error: Expected function, string, or
    # symbol"; they must now resolve without throwing a TypeError.
    @test methods(Int64) isa AbstractVector
    @test methods(Int64, Tuple{Float64}) isa AbstractVector
    @test methods(Float64) isa AbstractVector

    # Single-argument infer_effects / infer_exception_type on a type callable
    # must reach the classifier instead of erroring.
    @test Base.infer_effects(Int64).nothrow isa Bool
    @test Base.infer_exception_type(Int64) isa Type
    @test Base.infer_effects(Float64).nothrow isa Bool
    @test Base.infer_exception_type(Float64) isa Type
end

true
