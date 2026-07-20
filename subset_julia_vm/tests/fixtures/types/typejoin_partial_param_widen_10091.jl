# typejoin: widen only the differing parametric params, keep agreeing ones
# (Issue #10091, tech-debt epic #10049).
#
# Upstream `julia/base/promotion.jl`'s same-name-wrapper `typejoin` loop
# (~lines 100-140) widens ONLY the parameter positions that differ between
# the two inputs to a fresh `where`-bound TypeVar, and keeps every parameter
# that agrees, producing a partial `UnionAll` -- it never collapses straight
# to the bare wrapper unless every position differs. sjulia's same-typename
# branch used to use an all-or-nothing `same` flag that discarded every
# parameter (even the agreeing ones) the moment ANY position differed.
#
# Some of these joins have a differing TRAILING type parameter; upstream
# elides a trailing free `where` clause in `show` (`Foo{Int64}` instead of
# `Foo{Int64, B} where B`) -- a separate, known sjulia display-layer gap
# (Issue #10195) unrelated to typejoin's lattice computation. Those cases are
# asserted via subtyping (the actual lattice property the epic cares about)
# instead of an exact display string, so this fixture is unaffected by the
# display gap. (struct definitions are hoisted to the top level rather than
# nested inside @testset blocks: Issue #10194.)
#
# DEPENDENT-BOUND follow-up (Issue #10091 follow-up): a later type parameter
# whose DECLARED BOUND references an earlier one (e.g. `struct Dep2{A,
# B<:A}`) needs the earlier position's resolved value substituted into the
# later position's bound, not just kept/widened independently -- otherwise
# the join escapes a free TypeVar and is not even a valid upper bound
# (`Dep2{Number,Int64} <: result` was `false`). `DepPair10091`/
# `DepTriple10091` below use their OWN type-parameter names (`P,Q` /
# `X,Y,Z`), disjoint from `PairT10091`/`Triple10091`/`MultiDims10091`'s
# (`A,B,C` / `N,T`). Issue #10252 later fixed the underlying cross-wrapper
# identity-cache collision; the dedicated regression below intentionally reuses
# `A,B,C` across two independent dependent-bound structs in one process.

using Test

struct PairT10091{A,B}
    a::A
    b::B
end

struct Triple10091{A,B,C}
    a::A
    b::B
    c::C
end

# N is a plain value parameter (not itself a Type), analogous to an
# array-rank-like `N::Int`; T is a Type parameter that agrees.
struct MultiDims10091{N,T}
    x::N
    y::T
end

# Dependent bound: Q must be a subtype of whatever P resolves to.
struct DepPair10091{P, Q<:P}
    p::P
    q::Q
end

# Two-level dependent chain: Y<:X, Z<:Y.
struct DepTriple10091{X, Y<:X, Z<:Y}
    x::X
    y::Y
    z::Z
end

struct DepPair10252{A, B<:A}
    a::A
    b::B
end

struct DepTriple10252{A, B<:A, C<:B}
    a::A
    b::B
    c::C
end

@testset "typejoin - partial parametric widening (Issue #10091 MWE)" begin
    # MWE row 1: trailing param (B) differs, leading param (A) agrees.
    # Upstream: `PairT10091{Int64}` == `PairT10091{Int64, B} where B`.
    r1 = typejoin(PairT10091{Int64,Int64}, PairT10091{Int64,String})
    @test PairT10091{Int64,Bool} <: r1      # both inputs' shape covered
    @test PairT10091{Int64,Float64} <: r1   # any B is covered
    @test !(PairT10091{Float64,Bool} <: r1) # A is pinned to Int64, not widened to Any
    @test r1 <: PairT10091                  # still bounded by the wrapper

    # MWE row 2: leading param (A) differs, trailing param (B=Float64) agrees.
    # Upstream: `PairT10091{A, Float64} where A` (prints in full).
    r2 = typejoin(PairT10091{Int64,Float64}, PairT10091{String,Float64})
    @test r2 === (PairT10091{A,Float64} where {A})
    @test PairT10091{Bool,Float64} <: r2
    @test !(PairT10091{Bool,Int64} <: r2)   # B is pinned to Float64
end

@testset "typejoin - 3-param case, differing middle position (Issue #10091)" begin
    r3 = typejoin(Triple10091{Int64,Int64,Int64}, Triple10091{Int64,String,Int64})
    @test r3 === (Triple10091{Int64,B,Int64} where {B})
    @test Triple10091{Int64,Bool,Int64} <: r3
    @test !(Triple10091{Float64,Bool,Int64} <: r3)  # A and C are pinned
    @test !(Triple10091{Int64,Bool,Float64} <: r3)
end

@testset "typejoin - value-kind (non-Type) parameter differs (Issue #10091)" begin
    r4 = typejoin(MultiDims10091{2,Int64}, MultiDims10091{3,Int64})
    @test r4 === (MultiDims10091{N,Int64} where {N})
    @test MultiDims10091{5,Int64} <: r4
    @test !(MultiDims10091{5,Float64} <: r4)  # T is pinned to Int64
end

@testset "typejoin - all params agree is unaffected (Issue #10091 regression guard)" begin
    @test typejoin(PairT10091{Int64,Int64}, PairT10091{Int64,Int64}) === PairT10091{Int64,Int64}
end

@testset "typejoin - dependent bound: leading param agrees, dependent trailing param differs (Issue #10091 follow-up)" begin
    # Upstream: `typejoin(Dep2{Number,Int64}, Dep2{Number,Float64})` ===
    # `Dep2{Number, B} where B<:Number` -- Q's declared bound `Q<:P` must
    # substitute P's resolved value (Number), not keep the stale `Q<:P`.
    r1 = typejoin(DepPair10091{Number,Int64}, DepPair10091{Number,Float64})
    @test r1 === (DepPair10091{Number,Q} where {Q<:Number})
    @test DepPair10091{Number,Int64} <: r1
    @test DepPair10091{Number,Float64} <: r1
    @test DepPair10091{Number,Bool} <: r1     # any Q<:Number is covered
    @test !(DepPair10091{Int64,Int64} <: r1)  # P is pinned to Number
end

@testset "typejoin - dependent bound: both params differ (Issue #10091 follow-up)" begin
    # Upstream: `typejoin(Dep2{Int64,Int64}, Dep2{Float64,Float64})` ===
    # `Dep2` (the bare wrapper) -- when the bound-referenced position ALSO
    # differs, there is nothing concrete left to substitute.
    r2 = typejoin(DepPair10091{Int64,Int64}, DepPair10091{Float64,Float64})
    @test r2 === DepPair10091
    @test DepPair10091{Int64,Int64} <: r2
    @test DepPair10091{Float64,Float64} <: r2
end

@testset "typejoin - dependent bound: 3-param chain, only trailing differs (Issue #10091 follow-up)" begin
    # Upstream: `typejoin(Dep3{Number,Real,Int64}, Dep3{Number,Real,Float64})`
    # === `Dep3{Number, Real, C} where C<:Real` -- Z's declared bound `Z<:Y`
    # substitutes Y's resolved (agreeing, concrete) value Real.
    r3 = typejoin(DepTriple10091{Number,Real,Int64}, DepTriple10091{Number,Real,Float64})
    @test r3 === (DepTriple10091{Number,Real,Z} where {Z<:Real})
    @test DepTriple10091{Number,Real,Int64} <: r3
    @test DepTriple10091{Number,Real,Float64} <: r3
end

@testset "typejoin - dependent bound: 3-param chain, middle AND trailing differ (Issue #10091 follow-up)" begin
    # Upstream: `typejoin(Dep3{Number,Int64,Int64}, Dep3{Number,Float64,Float64})`
    # === `Dep3{Number, B, C} where {B<:Number, C<:B}` -- the middle position
    # (Y) differs and widens to a fresh TypeVar bounded by X's resolved value
    # (Number), and the trailing position (Z) differs too, widening to a
    # fresh TypeVar bounded by Y's OWN freshly-rebound TypeVar (not the stale
    # original) -- a 2-level dependency chain propagated through the widening.
    r4 = typejoin(DepTriple10091{Number,Int64,Int64}, DepTriple10091{Number,Float64,Float64})
    @test r4 === (DepTriple10091{Number,Y,Z} where {Y<:Number, Z<:Y})
    @test DepTriple10091{Number,Int64,Int64} <: r4
    @test DepTriple10091{Number,Float64,Float64} <: r4
    @test DepTriple10091{Number,Bool,Bool} <: r4
    @test !(DepTriple10091{Int64,Int64,Int64} <: r4)  # X is pinned to Number
end

@testset "typejoin - dependent bound TypeVar identities are wrapper-local (Issue #10252)" begin
    # Both structs deliberately reuse the same parameter names. A previous
    # name-keyed RuntimeTypeVar cache let DepPair10252's `A`/`B` values leak into
    # DepTriple10252's generic wrapper walk, so the second join collapsed to the
    # bare wrapper `DepTriple10252` instead of preserving the leading `Number`.
    r2 = typejoin(DepPair10252{Int64,Int64}, DepPair10252{Float64,Float64})
    @test r2 === DepPair10252

    r4 = typejoin(DepTriple10252{Number,Int64,Int64}, DepTriple10252{Number,Float64,Float64})
    @test r4 === (DepTriple10252{Number,B,C} where {B<:Number, C<:B})
    @test DepTriple10252{Number,Int64,Int64} <: r4
    @test DepTriple10252{Number,Float64,Float64} <: r4
    @test DepTriple10252{Number,Bool,Bool} <: r4
    @test !(DepTriple10252{Int64,Int64,Int64} <: r4)
end

@testset "typejoin - a partial UnionAll result fed back in as an operand (Issue #10091 codex review)" begin
    # A `reduce`/`mapreduce`-style accumulation feeds a PREVIOUS typejoin
    # result back in as an operand of the NEXT call. Once that result is a
    # partial UnionAll (this PR's own new output shape), typejoin must
    # unwrap it (mirroring upstream's `isa(a, UnionAll)` check ahead of the
    # DataType-assuming Tuple/Array/same-typename paths, which all index
    # `.parameters` -- not meaningful on a raw UnionAll) instead of falling
    # through to the DataType supertype-chain walk and over-widening to Any.
    r1 = typejoin(PairT10091{Int64,Int64}, PairT10091{Int64,String})
    @test PairT10091{Int64,Int64} <: r1
    @test PairT10091{Int64,String} <: r1
    r2 = typejoin(r1, PairT10091{Float64,String})
    @test r2 === PairT10091
    @test PairT10091{Float64,String} <: r2
    @test PairT10091{Int64,Int64} <: r2
end

true
