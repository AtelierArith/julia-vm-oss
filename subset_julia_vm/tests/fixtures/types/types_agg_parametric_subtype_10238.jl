# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: types/abstract_irrational_hierarchy.jl =====
module Agg_abstract_irrational_hierarchy
# AbstractIrrational <: Real type hierarchy parity (Issue #5134)
#
# Verifies the `AbstractIrrational <: Real` abstract type hierarchy that backs
# Irrational dispatch/promote. Every assertion below matches upstream Julia
# (`julia/base/irrationals.jl`) exactly, so this fixture passes under both
# `sjulia` and official `julia`.
#
# NOTE: This fixture deliberately does NOT assert `typeof(pi)`/`pi isa
# Irrational`. In this VM `pi` is folded to a `Float64` literal by design
# (Issue #533); promoting the built-in `pi` constant to an actual
# `Irrational{:π}` value is tracked separately. Here we only exercise the
# abstract hierarchy itself, which is what Issue #5134 requests.

using Test

# User-defined subtype declared at top level (struct defs cannot be nested
# inside a @testset begin ... end block in this subset).
struct MyIrr{sym} <: AbstractIrrational end
classify(::AbstractIrrational) = "irrational"
classify(::Real) = "real"

@testset "AbstractIrrational <: Real hierarchy (Issue #5134)" begin
    # --- Abstract hierarchy: AbstractIrrational <: Real <: Number ---
    @test AbstractIrrational <: Real
    @test AbstractIrrational <: Number
    @test AbstractIrrational <: Any
    @test supertype(AbstractIrrational) == Real

    # --- Irrational{sym} <: AbstractIrrational ---
    @test Irrational <: AbstractIrrational
    @test Irrational <: Real
    @test Irrational <: Number
    @test supertype(Irrational) == AbstractIrrational

    # --- Parametric instantiations preserve the hierarchy ---
    @test Irrational{:π} <: AbstractIrrational
    @test Irrational{:π} <: Real
    @test Irrational{:π} <: Number
    @test Irrational{:π} <: Irrational
    @test !(Irrational{:e} <: Irrational{:π})
    @test Irrational{:π} <: Irrational{:π}

    # --- A constructed Irrational value satisfies isa across the hierarchy ---
    x = Irrational{:π}()
    @test typeof(x) == Irrational{:π}
    @test x isa Irrational{:π}
    @test x isa Irrational
    @test x isa AbstractIrrational
    @test x isa Real
    @test x isa Number

    # --- The exported `pi` constant is a Real (Issue #5134 fixture) ---
    @test pi isa Real

    # --- User-defined subtypes participate in dispatch as Real ---
    @test MyIrr{:sqrt2} <: AbstractIrrational
    @test MyIrr{:sqrt2} <: Real
    @test MyIrr{:sqrt2}() isa AbstractIrrational
    @test MyIrr{:sqrt2}() isa Real

    # Bug #5582 / broader method specificity work (#5072): the narrower
    # AbstractIrrational method must beat the broader Real fallback.
    @test classify(Irrational{:π}()) == "irrational"
    @test classify(MyIrr{:sqrt2}()) == "irrational"
    @test classify(1.5) == "real"

    # Tuple parametric covariance through the hierarchy
    @test Tuple{Irrational{:π}} <: Tuple{AbstractIrrational}
    @test Tuple{Irrational{:π}} <: Tuple{Real}
end

# Return true to indicate success
end # module Agg_abstract_irrational_hierarchy

# ===== source: types/abstractarray_parent_param_chain_7728.jl =====
module Agg_abstractarray_parent_param_chain_7728
# Issue #7728: a value/type-parameter chain through an `AbstractArray{T,N}`
# parent across an abstract supertype chain must thread the concrete element
# and dimension parameters down to the AbstractArray instantiation.
#
# StaticArrays-shaped hierarchy:
#   StaticArray7728{S,T,N} <: AbstractArray{T,N}
#   StaticVector7728{N,T}  <: StaticArray7728{Tuple{N},T,1}
#   SVector7728{N,T}       <: StaticVector7728{N,T}
#
# sjulia previously dropped the parametric PARENT's parameters when lowering an
# `abstract type ... <: Parent{...}` declaration (only the parent base name was
# kept), so the subtype machinery could not substitute T=Int64, N=1 down the
# abstract chain and `SVector7728{3,Int64} <: AbstractArray{Int64,1}` was
# wrongly false. The direct and abstract-name links already worked.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

abstract type StaticArray7728{S,T,N} <: AbstractArray{T,N} end
abstract type StaticVector7728{N,T} <: StaticArray7728{Tuple{N},T,1} end
struct SVector7728{N,T} <: StaticVector7728{N,T}
    data::Tuple
end

@testset "AbstractArray parent param chain (Issue #7728)" begin
    # Direct and abstract-name links (already worked before the fix).
    @test SVector7728{3,Int64} <: StaticVector7728{3,Int64}
    @test SVector7728{3,Int64} <: StaticArray7728

    # The bug: the parameterized AbstractArray check must thread T=Int64, N=1.
    @test SVector7728{3,Int64} <: AbstractArray{Int64,1}
    @test SVector7728{2,Float64} <: AbstractArray{Float64,1}

    # Element/dimension parameters are invariant: a different element type or
    # rank is NOT a subtype.
    @test !(SVector7728{3,Int64} <: AbstractArray{Float64,1})
    @test !(SVector7728{3,Int64} <: AbstractArray{Int64,2})

    # Intermediate abstract links in the chain also carry their parameters.
    @test SVector7728{3,Int64} <: StaticArray7728{Tuple{3},Int64,1}
    @test !(SVector7728{3,Int64} <: StaticArray7728{Tuple{3},Float64,1})
end
end # module Agg_abstractarray_parent_param_chain_7728

# ===== source: types/bare_abstractarray_user_chain_7787.jl =====
module Agg_bare_abstractarray_user_chain_7787
# Issue #7787: a user type whose declared abstract parent chain reaches
# `AbstractArray{T,N}` must be `<:` the BARE, parameter-free `AbstractArray`
# (and the bare `AbstractVector`/`AbstractMatrix` when its rank matches), not
# just the parameterized `AbstractArray{T}` form (which was fixed in #7728).
#
# Before the fix, the bare-abstract array arms of
# `struct_is_subtype_of_abstract_with_lookup` only matched BUILT-IN array-family
# NAMES; they did not walk the user struct/abstract hierarchy up into the array
# family, so `MyArr{Float64} <: AbstractArray` was wrongly `false` while the
# parameterized `MyArr{Float64} <: AbstractArray{Float64}` was already `true`.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

# Rank-2 chain (parent is AbstractArray{T,2}).
abstract type AbsContainer7787{T} <: AbstractArray{T,2} end
struct MyArr7787{T} <: AbsContainer7787{T}
    data::Tuple
end

# Rank-1 chain (parent is AbstractVector{T} == AbstractArray{T,1}).
abstract type AbsVecContainer7787{T} <: AbstractVector{T} end
struct MyVecArr7787{T} <: AbsVecContainer7787{T}
    data::Tuple
end

# DenseArray-rooted rank-1 chain.
abstract type AbsDense7787{T} <: DenseArray{T,1} end
struct MyDense7787{T} <: AbsDense7787{T}
    data::Tuple
end

# A user type that is NOT an array must stay outside the array family.
struct Plain7787
    x::Int
end

@testset "bare AbstractArray over user chain (Issue #7787)" begin
    # The bug: bare, parameter-free AbstractArray over a user chain.
    @test MyArr7787{Float64} <: AbstractArray
    # The parameterized form already worked (Issue #7728); keep it green.
    @test MyArr7787{Float64} <: AbstractArray{Float64}

    # Rank: the parent pins rank 2, so the bare AbstractMatrix matches but the
    # bare AbstractVector does not.
    @test MyArr7787{Float64} <: AbstractMatrix
    @test !(MyArr7787{Float64} <: AbstractVector)
    @test MyArr7787{Float64} <: AbstractMatrix{Float64}
    @test !(MyArr7787{Float64} <: AbstractVector{Float64})

    # DenseArray is more specific than AbstractArray: an AbstractArray-rooted
    # user type is NOT a DenseArray.
    @test !(MyArr7787{Float64} <: DenseArray)
    @test !(MyArr7787{Float64} <: DenseArray{Float64})

    # Rank-1 chain: bare AbstractVector matches, AbstractMatrix does not.
    @test MyVecArr7787{Int} <: AbstractArray
    @test MyVecArr7787{Int} <: AbstractVector
    @test !(MyVecArr7787{Int} <: AbstractMatrix)
    @test !(MyVecArr7787{Int} <: DenseArray)

    # DenseArray-rooted rank-1 chain: DenseArray AND AbstractVector match.
    @test MyDense7787{Int} <: AbstractArray
    @test MyDense7787{Int} <: DenseArray
    @test MyDense7787{Int} <: AbstractVector
    @test !(MyDense7787{Int} <: AbstractMatrix)

    # Non-array user type stays outside the array family.
    @test !(Plain7787 <: AbstractArray)
    @test !(Plain7787 <: DenseArray)
    @test !(Plain7787 <: AbstractVector)
end
end # module Agg_bare_abstractarray_user_chain_7787

# ===== source: types/diagonal_rule.jl =====
module Agg_diagonal_rule
# Test Diagonal Rule for type parameter dispatch (Issue #2554)
# When a type variable T appears more than once in covariant position
# and never in invariant position, T must bind to a concrete type.

using Test

# Diagonal Rule applies: T appears twice in Tuple (covariant position)
function sum_pair(t::Tuple{T, T}) where T
    return t[1] + t[2]
end

# Diagonal Rule does NOT apply: T and S are different type variables
function diff_pair(t::Tuple{T, S}) where {T, S}
    return (t[1], t[2])
end

# Diagonal Rule does NOT apply: T appears only once
function first_elem(t::Tuple{T, String}) where T
    return t[1]
end

# Diagonal Rule applies: T appears twice in function parameters (covariant)
function same_type(x::T, y::T) where T
    return (x, y)
end

# Diagonal Rule does NOT apply: different type variables
function diff_type(x::T, y::S) where {T, S}
    return (x, y)
end

@testset "Diagonal Rule (Issue #2554)" begin
    @testset "Tuple{T, T}: concrete types match" begin
        # T=Int64, concrete → OK
        @test sum_pair((1, 2)) == 3
        @test sum_pair((10, 20)) == 30

        # T=Float64, concrete → OK
        @test sum_pair((1.5, 2.5)) == 4.0
    end

    @testset "Function params: same concrete type matches" begin
        # T=Int64 for both → OK
        @test same_type(1, 2) == (1, 2)

        # T=Float64 for both → OK
        @test same_type(1.0, 2.0) == (1.0, 2.0)

        # T=String for both → OK
        @test same_type("a", "b") == ("a", "b")
    end

    @testset "Different type variables: no diagonal rule" begin
        # Different type variables → diagonal rule does not apply
        @test diff_pair((1, "hello")) == (1, "hello")
        @test diff_type(1, "hello") == (1, "hello")
        @test diff_type(1, 2.0) == (1, 2.0)
    end

    @testset "Single occurrence: no diagonal rule" begin
        # T appears once → any type accepted
        @test first_elem((42, "answer")) == 42
        @test first_elem((3.14, "pi")) == 3.14
    end
end
end # module Agg_diagonal_rule

# ===== source: types/irrational_bare_family_subtype_5853.jl =====
module Agg_irrational_bare_family_subtype_5853
using Test

@test Irrational{:π} <: Irrational
@test Irrational{:ℯ} <: Irrational
@test !(Irrational{:π} <: Rational)
end # module Agg_irrational_bare_family_subtype_5853

# ===== source: types/lower_bound_typevar_roundtrip_9627.jl =====
module Agg_lower_bound_typevar_roundtrip_9627
# Lower-bound TypeVar conversion and rendering should preserve both bounds (Issue #9627)

using Test

abstract type LBBox9627{T} end
struct LBItem9627{T} <: LBBox9627{T}
    value::T
end

@testset "Lower-bound TypeVar round-trip prevention" begin
    @test LBItem9627{Integer} <: (LBBox9627{T} where Int64<:T<:Real)
    @test !(LBItem9627{Any} <: (LBBox9627{T} where Int64<:T<:Real))

    @test Vector{Integer} <: (Vector{T} where Int64<:T<:Real)
    @test !(Vector{Any} <: (Vector{T} where Int64<:T<:Real))

    @test string(Vector{>:Int64}) == "Vector{>:Int64}"
    @test string(Vector{T} where T>:Int64) == "Vector{T} where T>:Int64"
    @test string(Vector{T} where Int64<:T<:Real) == "Vector{T} where Int64<:T<:Real"
end
end # module Agg_lower_bound_typevar_roundtrip_9627

# ===== source: types/nested_alias_param_subtype_5047.jl =====
module Agg_nested_alias_param_subtype_5047
# Nested / alias-spelled parametric parameters in the exists-right `where`
# subtype solver (Advances Issue #5047).
#
# When a parametric type's parameter is itself parametric (`Box{Box{Int}}`), the
# rendered type name carries the 64-bit word alias `Int` (not the canonical
# `Int64`) on the NESTED level. The structured CoreType subtype engine parses
# operand names with `from_julia_name`, where a bare `Int` (no dedicated arm)
# becomes an opaque `Named("Int")` that is `<:` nothing. A bound check against
# such a parameter — e.g. `Box{Box{Int}} <: (Box{Box{T}} where T<:Integer)` —
# therefore wrongly returned `false`, even though the equivalent explicit-`Int64`
# spelling worked and upstream Julia returns `true`.
#
# The subtype relation now resolves a `Named("Int")`/`Named("UInt")` to its
# concrete word primitive (`Int64`/`UInt64`) so the exists-right matcher runs the
# bound check on the primitive. This is confined to subtyping — `from_julia_name`
# keeps the `Named` spelling, so type-propagation/dispatch are unchanged.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

struct Box{T}
    x::T
end

@testset "nested alias param: bounded exists-right (Issue #5047)" begin
    @test (Box{Box{Int}} <: (Box{Box{T}} where T<:Integer)) == true
    @test (Box{Box{Int}} <: (Box{Box{T}} where T<:Number)) == true
    @test (Box{Box{Int}} <: (Box{Box{T}} where T<:Real)) == true
    # element fails the bound -> rejected
    @test (Box{Box{String}} <: (Box{Box{T}} where T<:Integer)) == false
    @test (Box{Box{Float64}} <: (Box{Box{T}} where T<:Integer)) == false
end

@testset "nested alias param: UInt word alias (Issue #5047)" begin
    @test (Box{Box{UInt}} <: (Box{Box{T}} where T<:Unsigned)) == true
    @test (Box{Box{UInt}} <: (Box{Box{T}} where T<:Integer)) == true
    @test (Box{Box{UInt}} <: (Box{Box{T}} where T<:Signed)) == false
end

@testset "alias in user where-bound clause (Issue #5047)" begin
    @test (Box{Int} <: (Box{T} where T<:Int)) == true
    @test (Box{UInt} <: (Box{T} where T<:UInt)) == true
    @test (Box{Float64} <: (Box{T} where T<:Int)) == false
end

@testset "deeper nesting + diagonal stay correct (Issue #5047)" begin
    @test (Box{Box{Box{Int}}} <: (Box{Box{Box{T}}} where T<:Integer)) == true
    @test (Box{Box{Box{String}}} <: (Box{Box{Box{T}}} where T<:Integer)) == false
end

# --- MUST STAY CORRECT: explicit-Int64 spelling, unbounded, non-where. ---
@testset "regression guard (Issue #5047)" begin
    # Explicit canonical spelling was always correct and must stay so.
    @test (Box{Box{Int64}} <: (Box{Box{T}} where T<:Integer)) == true
    # Unbounded `where T` accepts any nested element.
    @test (Box{Box{Int}} <: (Box{Box{T}} where T)) == true
    @test (Box{Box{String}} <: (Box{Box{T}} where T)) == true
    # Plain invariant / shape mismatches.
    @test (Box{Int} <: Box{Real}) == false
    @test (Box{Int} <: Box{Int}) == true
    @test (Box{Box{Int}} <: Box{Box{String}}) == false
end
end # module Agg_nested_alias_param_subtype_5047

# ===== source: types/nested_diagonal_rule_5050.jl =====
module Agg_nested_diagonal_rule_5050
# Issue #5050: enforce the diagonal rule for nested covariant type-variable
# occurrences. A `where` variable that appears in a parametric parameter such
# as `Vector{T}` (its element position is covariant) together with a bare `T`
# must bind to a single concrete type across the matched arguments. Upstream
# Julia rejects `nest([1, 2], 3.0)` for `nest(x::Vector{T}, y::T) where T`
# because `T` would have to be both `Int64` and `Float64`.
using Test

nest(x::Vector{T}, y::T) where T = "match"
nest(x, y) = "fallback"

mnest(x::Matrix{T}, y::T) where T = "match"
mnest(x, y) = "fallback"

tri(a::Vector{T}, b::Vector{T}, c::T) where T = "match"
tri(a, b, c) = "fallback"

@testset "Issue #5050 nested diagonal rule" begin
    # Vector{T} element type must equal the bare T argument.
    @test nest([1, 2], 3) == "match"
    @test nest([1.0, 2.0], 3.0) == "match"
    @test nest([1, 2], 3.0) == "fallback"
    @test nest([1.0, 2.0], 3) == "fallback"

    # Matrix{T} element type must equal the bare T argument.
    @test mnest([1 2; 3 4], 5) == "match"
    @test mnest([1 2; 3 4], 5.0) == "fallback"

    # Multiple Vector{T} parameters plus a bare T must all agree.
    @test tri([1, 2], [3, 4], 5) == "match"
    @test tri([1, 2], [3, 4], 5.0) == "fallback"
    @test tri([1, 2], [3.0, 4.0], 5) == "fallback"
end
end # module Agg_nested_diagonal_rule_5050

# ===== source: types/pairs_parametric_abstractdict_parent_5882.jl =====
module Agg_pairs_parametric_abstractdict_parent_5882
using Test

import Base: Pairs

const P = Pairs{Symbol,Int64,Tuple{Symbol},NamedTuple{(:a,),Tuple{Int64}}}

@test P <: AbstractDict
@test P <: AbstractDict{Symbol,Int64}
@test !(P <: AbstractDict{Symbol,Any})
@test !(P <: AbstractDict{Any,Int64})
end # module Agg_pairs_parametric_abstractdict_parent_5882

# ===== source: types/parametric_exists_right_5615.jl =====
module Agg_parametric_exists_right_5615
# Issue #5615: a user parametric struct with a PARAMETRIC abstract parent
# (`struct MyVec{T} <: Wrapper{T}`) must subtype an EXISTENTIAL parametric right
# operand (`Wrapper{S} where S`) from a forall/bare left. The runtime reflection
# supertype of such a struct is its param-erased base (or `Any`), losing the
# invariant element binding, so the supertype-chain walk could not reach
# `Wrapper{T}`. It now consults the declared parametric parent TEMPLATE
# (`Wrapper{T} where T`) and re-enters the structured CoreType solver, while a
# concrete invariant parent (`Wrapper{Real}`) correctly stays false.

using Test

abstract type Wrapper5615{S} end
struct MyVec5615{T} <: Wrapper5615{T} end

@testset "parametric struct <: existential parametric parent (Issue #5615)" begin
    # forall-left / bare-left against an existential right → true
    @test (MyVec5615{T} where T) <: (Wrapper5615{S} where S)
    @test (MyVec5615{T} where T <: Real) <: (Wrapper5615{S} where S)
    @test MyVec5615 <: (Wrapper5615{S} where S)

    # concrete-left and bare-right already held; keep them green
    @test MyVec5615{Int} <: (Wrapper5615{S} where S)
    @test (MyVec5615{T} where T) <: Wrapper5615

    # element invariance is preserved: a concrete parent instantiation does NOT
    # match a forall-left, and a mismatched element stays false
    @test !((MyVec5615{T} where T) <: Wrapper5615{Real})
    @test MyVec5615{Int} <: Wrapper5615{Int}
    @test !(MyVec5615{Int} <: Wrapper5615{Real})
end
end # module Agg_parametric_exists_right_5615

# ===== source: types/range_abstract_array_hierarchy_5880.jl =====
module Agg_range_abstract_array_hierarchy_5880
using Test

import Base: LogRange

@testset "range abstract array hierarchy (Issues #5615/#5880)" begin
    @test AbstractRange <: AbstractVector
    @test AbstractRange <: AbstractArray
    @test AbstractUnitRange <: AbstractRange
    @test AbstractUnitRange <: AbstractVector
    @test AbstractUnitRange <: AbstractArray

    @test AbstractRange{Int64} <: AbstractVector{Int64}
    @test !(AbstractRange{Int64} <: AbstractVector{Integer})
    @test UnitRange{Int64} <: AbstractVector{Int64}
    @test UnitRange{Int64} <: AbstractArray{Int64,1}
    @test !(UnitRange{Int64} <: AbstractVector{Integer})
    @test !(UnitRange{Int64} <: Array{Int64,1})
    @test !(UnitRange{Int64} <: DenseArray{Int64,1})

    @test StepRangeLen{Float64} <: AbstractVector{Float64}
    @test LinRange{Float64} <: AbstractArray{Float64,1}
    @test LogRange{Float64} <: AbstractVector{Float64}
    @test LogRange{Float64} <: AbstractArray{Float64,1}
    @test !(LogRange{Float64} <: AbstractRange)
    @test !(LogRange{Float64} <: AbstractVector{Real})
end
end # module Agg_range_abstract_array_hierarchy_5880

# ===== source: types/range_core_subtype_gate_5615_5875.jl =====
module Agg_range_core_subtype_gate_5615_5875
using Test

import Base: LogRange, OneTo

@testset "range subtype CoreType gate (Issues #5615/#5875)" begin
    @test UnitRange <: AbstractUnitRange
    @test OneTo <: AbstractUnitRange
    @test UnitRange <: AbstractRange
    @test StepRange <: AbstractRange
    @test StepRangeLen <: AbstractRange
    @test LinRange <: AbstractRange

    @test UnitRange{Int64} <: AbstractUnitRange
    @test UnitRange{Int64} <: AbstractRange
    @test StepRange{Int64,Int64} <: AbstractRange
    @test StepRangeLen{Float64} <: AbstractRange
    @test LinRange{Float64} <: AbstractRange

    @test !(LogRange <: AbstractRange)
    @test !(LogRange{Float64} <: AbstractRange)
    @test !(LogRange{Float64} <: AbstractRange{Float64})
end
end # module Agg_range_core_subtype_gate_5615_5875

# ===== source: types/typejoin_parametric_5112.jl =====
module Agg_typejoin_parametric_5112
# typejoin for parametric Tuple types and same-name parametric structs (Issue #5112)

using Test

struct TJ5112Box{T}
    x::T
end

struct TJ9841Box{T}
    x::T
end

@testset "typejoin - parametric Tuple types (Issue #5112)" begin
    # Elementwise join of fixed-length Tuple types
    @test typejoin(Tuple{Int}, Tuple{Float64}) === Tuple{Real}
    @test typejoin(Tuple{Int,Int}, Tuple{Int,Float64}) === Tuple{Int64,Real}
    # Identical tuples are unchanged
    @test typejoin(Tuple{Int}, Tuple{Int}) === Tuple{Int64}
    @test typejoin(Tuple{Int,String}, Tuple{Int,String}) === Tuple{Int64,String}
end

@testset "typejoin - same-name parametric structs (Issue #5112)" begin
    # Differing parameters collapse to the base type
    @test typejoin(TJ5112Box{Int}, TJ5112Box{Float64}) === TJ5112Box
    # Identical instantiations are unchanged
    @test typejoin(TJ5112Box{Int}, TJ5112Box{Int}) === TJ5112Box{Int}
end

@testset "typejoin - concrete instance with UnionAll wrapper (Issue #9841)" begin
    @test typejoin(Complex{Float64}, Complex) === Complex
    @test typejoin(Complex, Complex{Float64}) === Complex
    @test typejoin(Rational{Int64}, Rational) === Rational
    @test typejoin(TJ9841Box{Int}, TJ9841Box) === TJ9841Box
    @test typejoin(TJ9841Box, TJ9841Box{Int}) === TJ9841Box
    @test promote_type(Complex{Float64}, Complex) === Complex
end

@testset "typejoin - existing scalar behaviour preserved" begin
    @test typejoin(Int64, Float64) === Real
    @test typejoin(Int64, Int64) === Int64
    @test typejoin(Int64, String) === Any
end
end # module Agg_typejoin_parametric_5112

# ===== source: types/types_pairs_abstractdict_subtype_5882.jl =====
module Agg_types_pairs_abstractdict_subtype_5882
# Issue #5882: `Base.Pairs{K,V,I,A}` declares `AbstractDict{K,V}` as its parametric
# abstract parent, so the parameterized subtype relation must thread K,V from the
# Pairs instantiation into AbstractDict{K,V}. Previously `supertype(Pairs{...})`
# returned `Any` (the builtin direct-supertype table hardcoded Pairs to `Any`,
# shadowing the pure-Julia `struct Pairs{K,V,I,A} <: AbstractDict{K,V}`), so the
# parameterized relation was false while the bare one was true.

using Test
import Base: Pairs

@testset "Pairs parametric parent threads into AbstractDict{K,V} (Issue #5882)" begin
    P = Pairs{Symbol,Int64,Tuple{Symbol},NamedTuple{(:a,),Tuple{Int64}}}

    @test (P <: AbstractDict) == true
    @test (P <: AbstractDict{Symbol,Int64}) == true
    @test (P <: AbstractDict{Symbol,Any}) == false
    @test (P <: AbstractDict{Any,Int64}) == false
    @test supertype(P) == AbstractDict{Symbol,Int64}
end

@testset "supertype regressions (Issue #5882)" begin
    @test supertype(Complex{Float64}) == Number
    @test supertype(Dict{String,Int64}) == AbstractDict{String,Int64}
    @test supertype(Vector{Int64}) == DenseVector{Int64}
end
end # module Agg_types_pairs_abstractdict_subtype_5882

# ===== source: types/types_struct_forall_left_abstract_parent_5614.jl =====
module Agg_types_struct_forall_left_abstract_parent_5614
using Test

# Issue #5614: a forall-left where-form over a user-defined PARAMETRIC struct
# must resolve its declared abstract parent. `(Circle{T} where T) <: Shape` is
# `true` upstream because every `Circle{T}` instantiation declares `Shape` as its
# supertype, independent of `T`. sjulia previously reported it `false`: the
# rendered `where` operand is decided authoritatively by the structured `CoreType`
# solver (it never falls through to the runtime reflection table that already
# handles the brace-free `Circle{Int} <: Shape`), and that solver both lacked a
# `(Struct, Named)` arm AND never received parametric user structs in its
# struct-parent registry (they instantiate lazily and live outside `struct_defs`,
# Issue #5052).

abstract type Shape end
struct Circle{T<:Real} <: Shape
    r::T
end
struct Square <: Shape end

abstract type Animal end
abstract type Mammal <: Animal end
struct Dog{T} <: Mammal
    name::T
end

abstract type Wrapper{T} end
struct MyVec{T} <: Wrapper{T}
    data::Vector{T}
end

@testset "forall-left parametric struct resolves abstract parent (Issue #5614)" begin
    # The bug: explicit where-form over a parametric struct.
    @test (Circle{T} where T) <: Shape
    @test (Circle{T} where T <: Real) <: Shape
    @test (Circle{T} where T <: Integer) <: Shape

    # Regressions: the brace-free / concrete forms already worked.
    @test Circle <: Shape
    @test Circle{Int} <: Shape
    @test Square <: Shape

    # Multi-level chain through an intermediate user abstract type.
    @test (Dog{T} where T) <: Mammal
    @test (Dog{T} where T) <: Animal
    @test Dog{Int} <: Animal
    @test Dog <: Animal
end

@testset "forall-left parametric struct with a parametric abstract parent (Issue #5614)" begin
    # `struct MyVec{T} <: Wrapper{T}`: the bare UnionAll is a subtype of the bare
    # parametric abstract...
    @test (MyVec{T} where T) <: Wrapper
    @test MyVec{Int} <: Wrapper
    @test MyVec{Int} <: Wrapper{Int}

    # ...but element invariance still holds: not every `MyVec{T}` is a
    # `Wrapper{Int}`, and an unrelated abstract never matches.
    @test !((MyVec{T} where T) <: Wrapper{Int})
    @test !((MyVec{T} where T) <: Shape)
    @test !((Dog{T} where T) <: Wrapper)
    @test !((Circle{T} where T) <: Animal)
end
end # module Agg_types_struct_forall_left_abstract_parent_5614

# ===== source: types/types_where_lower_bound_display_5650.jl =====
module Agg_types_where_lower_bound_display_5650
# Issue #5650: type display must carry where-clause LOWER bounds and normalize the
# contravariant `>:` shorthand.
#
# Previously `JuliaType::UnionAll` carried only a single (upper) bound, and the
# value-position `where` parser flattened `>:` / `Lower<:T<:Upper` constraints
# into a generic binary expression that dropped the lower bound; `Vector{>:Int}`
# also survived as an unnormalized raw string.

using Test

# where-clause lower bounds (single, double, and the existing upper-only form).
@test string(Vector{T} where Int<:T<:Real) == "Vector{T} where Int64<:T<:Real"
@test string(Vector{T} where T>:Int) == "Vector{T} where T>:Int64"
@test string(Vector{T} where T<:Real) == "Vector{T} where T<:Real"
@test string(Array{T} where Int8<:T<:Signed) == "Array{T} where Int8<:T<:Signed"

# Anonymous contravariant shorthand `>:Bound` with alias normalization (Int->Int64).
@test string(Vector{>:Int}) == "Vector{>:Int64}"
@test string(Vector{>:Integer}) == "Vector{>:Integer}"

# Regression: the covariant `<:Bound` shorthand and unbounded/concrete parametric
# types still render unchanged.
@test string(Vector{<:Real}) == "Vector{<:Real}"
@test string(Vector{Int}) == "Vector{Int64}"
end # module Agg_types_where_lower_bound_display_5650

# ===== source: types/user_nominal_core_subtype_gate_5615.jl =====
module Agg_user_nominal_core_subtype_gate_5615
using Test

abstract type Animal5615 end
abstract type Mammal5615 <: Animal5615 end
abstract type Vehicle5615 end

struct Dog5615 <: Mammal5615 end
struct Cat5615 <: Animal5615 end

@test Dog5615 <: Animal5615
@test Dog5615 <: Mammal5615
@test !(Cat5615 <: Mammal5615)
@test !(Dog5615 <: Vehicle5615)

@test Tuple{Dog5615} <: Tuple{Animal5615}
@test Tuple{Tuple{Dog5615}, Int64} <: Tuple{Tuple{Animal5615}, Real}
@test !(Tuple{Dog5615} <: Tuple{Vehicle5615})
@test !(Tuple{Cat5615} <: Tuple{Mammal5615})
end # module Agg_user_nominal_core_subtype_gate_5615

# ===== source: types/user_parametric_core_subtype_gate_5615.jl =====
module Agg_user_parametric_core_subtype_gate_5615
using Test

abstract type AnimalParam5615 end
abstract type VehicleParam5615 end

struct BoxParam5615{T} <: AnimalParam5615
    value::T
end

struct CrateParam5615{T} <: VehicleParam5615
    value::T
end

@test BoxParam5615{Int64} <: AnimalParam5615
@test BoxParam5615{String} <: AnimalParam5615
@test !(BoxParam5615{Int64} <: VehicleParam5615)
@test !(CrateParam5615{Int64} <: AnimalParam5615)

@test Tuple{BoxParam5615{Int64}} <: Tuple{AnimalParam5615}
@test Tuple{Tuple{BoxParam5615{String}}, Int64} <: Tuple{Tuple{AnimalParam5615}, Real}
@test !(Tuple{BoxParam5615{Int64}} <: Tuple{VehicleParam5615})
@test !(Tuple{CrateParam5615{Int64}} <: Tuple{AnimalParam5615})
end # module Agg_user_parametric_core_subtype_gate_5615

# ===== source: types/user_parametric_parent_where_right_5852.jl =====
module Agg_user_parametric_parent_where_right_5852
using Test

abstract type WrapperWhereRight5852{S} end
struct MyVecWhereRight5852{T} <: WrapperWhereRight5852{T}
    value::T
end

@test (MyVecWhereRight5852{T} where T) <: (WrapperWhereRight5852{S} where S)
@test MyVecWhereRight5852{Int64} <: (WrapperWhereRight5852{S} where S)
@test MyVecWhereRight5852{Int64} <: WrapperWhereRight5852{Int64}
@test !(MyVecWhereRight5852{Int64} <: WrapperWhereRight5852{Real})

@test MyVecWhereRight5852{Int64} <: (WrapperWhereRight5852{S} where S<:Real)
@test !(MyVecWhereRight5852{String} <: (WrapperWhereRight5852{S} where S<:Real))
end # module Agg_user_parametric_parent_where_right_5852

# ===== source: types/value_param_abstractarray_parent_chain_7819.jl =====
module Agg_value_param_abstractarray_parent_chain_7819
# Issue #7819 (follow-up to #7728): the AbstractArray{T,N} subtype edge must
# survive an EXTRA value-parameter intermediate that re-passes {S,T,N} to its
# parent — the exact shape StaticArrays uses:
#
#   StaticArray7819{S,T,N}    <: AbstractArray{T,N}
#   StaticVecOrMat7819{S,T,N} <: StaticArray7819{S,T,N}        # re-passes {S,T,N}
#   StaticVector7819{N,T}     <: StaticVecOrMat7819{Tuple{N},T,1}
#   Vec7819{N,T}              <: StaticVector7819{N,T}
#
# #7728's fixture only had a single intermediate. With the extra
# StaticVecOrMat7819 layer sjulia collapsed the parametric family entry's
# type-parameter list (a monomorphized instance clobbered it with an empty list),
# so `registered_instantiated_struct_parent_in` could no longer substitute the
# concrete arguments and EVERY edge in the chain went false. All expectations
# below were verified against upstream Julia 1.12.

using Test

abstract type StaticArray7819{S,T,N} <: AbstractArray{T,N} end
abstract type StaticVecOrMat7819{S,T,N} <: StaticArray7819{S,T,N} end
abstract type StaticVector7819{N,T} <: StaticVecOrMat7819{Tuple{N},T,1} end
struct Vec7819{N,T} <: StaticVector7819{N,T}
    data::Tuple
end

@testset "value-param AbstractArray parent chain w/ extra intermediate (Issue #7819)" begin
    @test Vec7819{3,Int64} <: StaticVector7819{3,Int64}
    @test Vec7819{3,Int64} <: StaticVecOrMat7819{Tuple{3},Int64,1}
    @test Vec7819{3,Int64} <: StaticArray7819{Tuple{3},Int64,1}
    @test Vec7819{3,Int64} <: StaticArray7819

    # The bug: the parameterized AbstractArray check must thread T=Int64, N=1
    # through TWO value-parameter intermediates.
    @test Vec7819{3,Int64} <: AbstractArray{Int64,1}
    @test Vec7819{3,Int64} <: AbstractArray

    # Element type / rank are invariant.
    @test !(Vec7819{3,Int64} <: AbstractArray{Float64,1})
    @test !(Vec7819{3,Int64} <: AbstractArray{Int64,2})
end
end # module Agg_value_param_abstractarray_parent_chain_7819

# ===== source: types/where_expression_value_5047.jl =====
module Agg_where_expression_value_5047
# `where`-expression in VALUE/expression position lowers to a UnionAll type
# value (Advances Issues #5047/#5049/#5053 — subtype-engine increment).
#
# Previously `Tuple{T,T} where T` and `Array{T,N} where {T,N}` in expression
# position failed at lowering with
# `UnsupportedFeature{UnsupportedExpression("where_expression")}`. This increment
# desugars `Body where {V...}` into nested `UnionAll(TypeVar(:V), Body)`
# construction, so the result is a first-class `UnionAll` type value: `typeof`
# is `UnionAll`, it `isa UnionAll`/`isa Type`, it displays correctly, and
# `Base.unwrap_unionall`/`rewrap_unionall` round-trip.
#
# OUT OF SCOPE (later increment): subtype SOLVING with `where`
# (e.g. `Tuple{Int,Int} <: (Tuple{T,T} where T)`), which needs the forall/exists
# solver. This fixture only asserts construction/typeof/display/identity/unwrap.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

@testset "where-expression as value lowers to UnionAll (Issue #5047)" begin
    # --- typeof of a where-expression value is UnionAll ---
    @test typeof(Tuple{T,T} where T) == UnionAll
    @test typeof(Array{T,N} where {T,N}) == UnionAll
    @test typeof(Vector{T} where T) == UnionAll

    # --- the value isa UnionAll / isa Type ---
    @test (Vector{T} where T) isa UnionAll
    @test (Tuple{T,T} where T) isa UnionAll
    @test (Tuple{T,T} where T) isa Type

    # --- bounded where in value position is still a UnionAll ---
    @test typeof(Vector{T} where T<:Number) == UnionAll
    @test (Vector{T} where T<:Number) isa UnionAll

    # --- unwrap_unionall peels the UnionAll layer back to the body ---
    # (The body has a free TypeVar, so we introspect it rather than writing a
    # bare `Tuple{T,T}` RHS — that would raise UndefVarError in plain Julia.)
    utt = (Tuple{T,T} where T)
    btt = Base.unwrap_unionall(utt)
    @test typeof(btt) == DataType
    @test btt isa DataType
    @test nameof(btt) == :Tuple
    @test string(btt) == "Tuple{T, T}"
    # rewrap_unionall round-trips: unwrap then rewrap recovers the value
    @test Base.rewrap_unionall(btt, utt) === utt
    uvt = (Vector{T} where T)
    @test Base.rewrap_unionall(Base.unwrap_unionall(uvt), uvt) isa UnionAll

    # --- equality/identity with the canonical builtin UnionAll aliases ---
    @test (Vector{T} where T) === Vector
    @test (Array{T,N} where {T,N}) === Array

    # --- alias-binding form: T1 = Array{T,N} where {T,N}; T1 === Array ---
    T1 = Array{T,N} where {T,N}
    @test T1 === Array

    # --- chained where display keeps the inner bound as the outer TypeVar name ---
    # Upstream prints `T<:S` under the outer `S<:Real` binder, not `T<:S<:Real`
    # (Issue #9721).
    chained = (Vector{T} where T<:S where S<:Real)
    @test string(chained) == "Vector{T} where {S<:Real, T<:S}"
end

# --- MUST STAY WORKING: declaration-position `where` is unaffected ---
f(x::T) where T = x
g(x::T) where T<:Number = x + 1
h(x::Vector{T}) where T = length(x)

@testset "declaration-position where still works (regression guard)" begin
    @test f(3) == 3
    @test g(3) == 4
    @test h([1, 2, 3]) == 3
end
end # module Agg_where_expression_value_5047

true
