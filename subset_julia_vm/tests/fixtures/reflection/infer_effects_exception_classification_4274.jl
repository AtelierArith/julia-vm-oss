using Test

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

true
