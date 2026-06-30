using Test

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

true
