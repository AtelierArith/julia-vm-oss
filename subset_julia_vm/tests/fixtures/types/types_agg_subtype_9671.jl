# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: types/array_unionall_core_subtype_gate_5615.jl =====

@testset "array UnionAll subtype CoreType gate (Issue #5615)" begin
    @test Vector{Int64} <: (Array{T} where T)
    @test Matrix{Float64} <: (Array{T} where T)
    @test Array{Bool,3} <: (Array{T} where T)

    @test Vector{Int64} <: Array{<:Real}
    @test !(Vector{String} <: Array{<:Real})
    @test !(Vector{Int64} <: Array{Real})

    @test Array{Float64,1} <: Vector{Float64}
    @test !(Array{Float64,2} <: Vector{Float64})
end

# ===== source: types/container_typevar_core_subtype_gate_5615.jl =====

@testset "container TypeVar subtype CoreType gate (Issues #5615/#5949)" begin
    @test Dict{String,Int64} <: (AbstractDict{String,T} where T)
    @test !(Dict{String,Int64} <: (AbstractDict{Symbol,T} where T))
    @test !(Dict{String,Int64} <: AbstractDict{String,Real})

    @test Set{Int64} <: (AbstractSet{T} where T)
    @test !(Set{String} <: (AbstractSet{T} where T<:Real))
    @test !(Set{Int64} <: AbstractSet{Real})

    @test Base.RefValue{Int64} <: (Ref{T} where T)
    @test !(Base.RefValue{String} <: (Ref{T} where T<:Real))
    @test !(Base.RefValue{Int64} <: Ref{Real})
end

# ===== source: types/coretype_parity_8415.jl =====

@testset "Core type parity regressions (Issue #8415)" begin
    @test typejoin(Tuple{Int,Int}, Tuple{Float64,Float64}) == Tuple{Real,Real}
    @test typejoin(Tuple{Int}, Tuple{Int,Int}) == Tuple{Int,Vararg{Int}}
    @test typejoin(Val{1}, Val{2}) == Val

    @test typeintersect(Tuple{T,T} where T, Tuple{Int,Float64}) == Union{}
    @test typeintersect(Tuple{T,T} where T, Tuple{Int,Real}) == Tuple{Int,Int}

    f_cross_bound_8415(x::T, y::S) where {T<:Real,S<:T} = (T, S)
    @test f_cross_bound_8415(1, 1) == (Int, Int)
    @test_throws Exception f_cross_bound_8415(1, 2.0)

    f_lower_bound_8415(x::T) where {T>:Int} = T
    @test f_lower_bound_8415(1) == Int
    @test_throws Exception f_lower_bound_8415(1.0)

    @test Vector{Int} <: (Vector{T} where T<:S where S<:Real)
    @test !(Vector{String} <: (Vector{T} where T<:S where S<:Real))

    @test typejoin(Vector{Int}, Matrix{Float64}) == Array
    @test typejoin(Vector{Int}, Matrix{Int}) == Array{Int}

    @test Vector{Int} === (Vector{T} where T){Int}
end

# ===== source: types/covariant_bound_type_arg_8352.jl =====
# Issue #8352: a covariant/contravariant bound shorthand inside `{}` —
# `Foo{<:Bound}` / `Foo{>:Bound}` (sugar for `Foo{T} where T<:Bound`) — must
# lower as a static bounded type expression, not be (mis)classified as a dynamic
# value parameter and routed through expression lowering, where the prefix `<:`
# is rejected with `UnsupportedOperator("<:")`. Regression introduced by #8339's
# change to `is_dynamic_type_arg`.


@testset "covariant/contravariant bound type args (Issue #8352)" begin
    # Lowering must succeed (these threw a lowering error before the fix).
    @test Vector{<:Real} isa Type
    @test Array{>:Int} isa Type

    # The bound participates in subtype queries.
    @test Vector{Int} <: Vector{<:Real}
    @test Vector{Float64} <: Vector{<:Real}
    @test !(Vector{String} <: Vector{<:Real})
    @test !(Type{<:Real} <: DataType)

    # As a method-argument annotation (the common use of the shorthand).
    g8352(::Type{<:Real}) = :real
    g8352(::Type{<:AbstractString}) = :str
    @test g8352(Int) === :real
    @test g8352(Float64) === :real
    @test g8352(String) === :str
end

# ===== source: types/eltype_union_array_5335.jl =====
# eltype of a Union-typed array must be a Union type object (Issue #5335)
#
# Previously the VM materialized the element type from an array's UnionOf tag as
# a `DataType`-tagged Struct("Union{...}") name rather than a real Union type
# object, so `eltype(v) == Union{...}` was false and `typeof(eltype(v))` was
# DataType instead of Union.


@testset "eltype of Union-typed array is a Union (Issue #5335)" begin
    v = Union{Int64,Float64}[1]
    @test eltype(v) == Union{Int64,Float64}
    @test string(typeof(eltype(v))) == "Union"
    # Union membership is order-independent.
    @test eltype(v) == Union{Float64,Int64}
    # Bare literal comparison (sanity, already worked).
    @test Union{Int64,Float64} == Union{Int64,Float64}
    # typeof of a bare union literal is also `Union`.
    @test string(typeof(Union{Int64,Float64})) == "Union"
    # A three-member union round-trips too.
    w = Union{Int64,Float64,String}[1]
    @test eltype(w) == Union{Int64,Float64,String}
    @test string(typeof(eltype(w))) == "Union"
end

# Return true to indicate success.

# ===== source: types/invariant_parametric_subtype_5047.jl =====
# Invariant parametric subtyping for built-in container abstract types
# (Advances Issue #5047 — first increment toward the unified subtype engine).
#
# Julia's parametric DataTypes/abstract array types are INVARIANT in their
# element parameter: `Vector{Float64} <: AbstractVector{Int64}` is false even
# though both are vectors, because the element type must be EQUAL (not merely a
# subtype). sjulia previously dropped the parameter of parametric *abstract*
# names (`AbstractVector{Int64}` was parsed as the bare `AbstractVector`), so
# the invariant parameter was never checked and these all wrongly returned true.
#
# All expectations below were verified against upstream Julia 1.12.


@testset "invariant parametric subtyping for builtin abstracts (Issue #5047)" begin
    # --- The bug: element parameter differs => false (was wrongly true) ---
    @test !(Vector{Float64} <: AbstractVector{Int64})
    @test !(Vector{Int} <: Vector{Real})
    @test !(AbstractVector{Int} <: AbstractVector{Real})
    @test !(Matrix{Float64} <: AbstractMatrix{Real})
    # Dict was already correct (stays Struct with exact-param equality)
    @test !(Dict{String,Int} <: Dict{String,Real})

    # --- Dimension parameter is also invariant (was wrongly true) ---
    @test !(Vector{Int} <: AbstractArray{Int,2})
    @test !(AbstractVector{Int} <: AbstractArray{Real})
    @test !(Vector{Float64} <: AbstractArray{Real})

    # --- Must stay correct: matching element parameter => true ---
    @test Vector{Int} <: AbstractVector{Int}
    @test Matrix{Int} <: AbstractMatrix{Int}
    @test Vector{Int} <: AbstractArray{Int,1}
    @test Vector{Int} <: AbstractArray{Int}
    @test Matrix{Int} <: AbstractArray{Int,2}
    @test Array{Int,1} <: AbstractVector{Int}
    @test Array{Int,2} <: AbstractMatrix{Int}
    @test AbstractVector{Int} <: AbstractArray{Int,1}
    @test AbstractVector{Int} <: AbstractArray{Int}
    @test AbstractMatrix{Int} <: AbstractArray{Int,2}

    # --- Must stay correct: bare (no-param) abstract supertype is covariant ---
    @test Vector{Int} <: AbstractVector
    @test Vector{Int} <: AbstractArray
    @test Matrix{Float64} <: AbstractArray
    @test Vector{Int} <: Vector{Int}
    @test Int <: Real

    # --- Must stay correct: wrong family => false ---
    @test !(Matrix{Int} <: AbstractVector{Int})
    @test !(Vector{Int} <: AbstractMatrix{Int})
end

# ===== source: types/nested_unionall_apply_5053.jl =====

@testset "multi-var UnionAll application (Issue #5053)" begin
    array_schema = Array{T,N} where {T,N}
    @test array_schema{Int,2} === Array{Int,2}
    @test array_schema{Float64,1} === Vector{Float64}
    @test Core.apply_type(array_schema, Int, 2) === Array{Int,2}

    tuple_schema = Tuple{T,U} where {T,U}
    @test tuple_schema{Int,String} === Tuple{Int,String}
    @test Core.apply_type(tuple_schema, Int, String) === Tuple{Int,String}

    nested_schema = Vector{Tuple{T,U}} where {T,U}
    @test nested_schema{Int,String} === Vector{Tuple{Int,String}}
    @test Core.apply_type(nested_schema, Int, String) === Vector{Tuple{Int,String}}

    # Uppercase aliases are ordinary UnionAll-valued bindings here, not static
    # type-alias declarations; applying them must still instantiate the nested
    # body variables.
    T2 = Vector{Tuple{T,U}} where {T,U}
    @test T2{Int,String} === Vector{Tuple{Int,String}}
    @test Core.apply_type(T2, Int, String) === Vector{Tuple{Int,String}}
end

# ===== source: types/subtype_abstract_supertype_tuple_5564.jl =====
# Subtype-engine: parametric struct `<:` its PARAMETRIZED abstract supertype,
# and covariant Tuple matching that honors element INVARIANCE (Issue #5564).
#
# Two gaps surfaced after the array-family invariant-subtype fix (#5563):
#
# Bug 1 — A parametric concrete container is a subtype of its PARAMETRIZED
# abstract supertype when the shared invariant parameters are EQUAL:
#   Dict{String,Int} <: AbstractDict{String,Int}  is true
#   Set{Int}         <: AbstractSet{Int}           is true
# #5563 wired this up for the array family (Vector → AbstractVector) but the
# non-array containers (Dict/AbstractDict, Set/AbstractSet) regressed to false
# once the old param-loss bug stopped masking them.
#
# Bug 2 — Covariant Tuple matching must use the (invariant-aware) element
# subtype check, so an invariant parametric element is compared by equality:
#   Tuple{Vector{Int}} <: Tuple{Vector{Real}}  is false  (Vector is invariant)
# while Tuples stay covariant in directly-related leaves:
#   Tuple{Int} <: Tuple{Real}  is true  (Int <: Real)
#
# All expectations below were verified against upstream Julia 1.12.


@testset "parametric struct <: parametrized abstract supertype (Issue #5564)" begin
    # --- Bug 1: regression — matching invariant params => true ---
    @test Dict{String,Int} <: AbstractDict{String,Int}
    @test Set{Int} <: AbstractSet{Int}
    # array family was already correct (#5563) — keep it green here too
    @test Vector{Int} <: AbstractVector{Int}

    # --- Must stay correct: differing invariant params => false ---
    @test !(Dict{String,Int} <: Dict{String,Real})
    @test !(Dict{String,Int} <: AbstractDict{String,Real})
    @test !(Set{Int} <: AbstractSet{Real})
    @test !(Vector{Float64} <: AbstractVector{Int64})

    # --- Must stay correct: bare (no-param) abstract supertype is covariant ---
    @test Dict{String,Int} <: AbstractDict
    @test Set{Int} <: AbstractSet

    # --- typeintersect consequences (covariant subtype keeps the subtype) ---
    @test typeintersect(Dict{String,Int}, AbstractDict{String,Int}) == Dict{String,Int}
    @test typeintersect(Set{Int}, AbstractSet{Int}) == Set{Int}
end

@testset "covariant Tuple honors element invariance (Issue #5564)" begin
    # --- Bug 2: invariant element under covariant Tuple => false ---
    @test !(Tuple{Vector{Int}} <: Tuple{Vector{Real}})
    @test !(Tuple{Int,Vector{Int}} <: Tuple{Real,Vector{Real}})

    # --- Must stay correct: Tuples ARE covariant in directly-related leaves ---
    @test Tuple{Int} <: Tuple{Real}
    @test Tuple{Vector{Int}} <: Tuple{Vector{Int}}
    @test Tuple{Int,Vector{Int}} <: Tuple{Real,Vector{Int}}

    # --- typeintersect consequence: invariant mismatch => Union{} ---
    @test typeintersect(Tuple{Vector{Int}}, Tuple{Vector{Real}}) == Union{}
end

# --- Must stay correct: the #5563 invariant array cases (do not re-break) ---
@testset "do not re-break #5563 invariant array subtyping" begin
    @test !(Vector{Int} <: Vector{Real})
    @test !(AbstractVector{Int} <: AbstractVector{Real})
    @test !(Vector{Int} <: AbstractArray{Int,2})
    @test Matrix{Int} <: AbstractMatrix{Int}
    @test Vector{Int} <: AbstractArray{Int}
end

# ===== source: types/subtype_concrete_not_bottom_5257.jl =====
# Test: Issue #5257 — `Nothing <: T` for unrelated concrete `T` must be false.
#
# `Nothing` (the type of `nothing`) is a normal concrete singleton DataType,
# NOT the bottom type `Union{}`. Only `Union{}` is `<:` everything. Previously
# the runtime `<:` path conflated the two, making `Nothing <: Int64` return
# `true`. Verified against upstream Julia 1.12.


@testset "Issue #5257: concrete-type subtype correctness" begin
    # Nothing is a concrete singleton type, NOT the bottom type.
    @test (Nothing <: Any) == true
    @test (Nothing <: Nothing) == true
    @test (Nothing <: Int64) == false
    @test (Nothing <: Union{Int64}) == false
    @test (Nothing <: Union{Int64, Float64}) == false
    @test (Nothing <: Union{Nothing, Int64}) == true

    # Only Union{} (Bottom) is a subtype of everything.
    @test (Union{} <: Int64) == true
    @test (Union{} <: Nothing) == true

    # Other concrete singletons behave the same.
    @test (Missing <: Int64) == false
    @test (Missing <: Any) == true

    # Unrelated concrete pairs.
    @test (Int64 <: Float64) == false
    @test (Int64 <: Nothing) == false
    @test (Int64 <: Real) == true
    @test (Int64 <: Number) == true
end

# ===== source: types/test_variance.jl =====
# Test type variance: Tuple is covariant, Array is invariant


@testset "Type variance" begin
    # Test 1: Tuple is COVARIANT
    # Tuple{Int64} <: Tuple{Number} because Int64 <: Number
    @test Tuple{Int64} <: Tuple{Number}
    @test Tuple{Int64} <: Tuple{Real}
    @test Tuple{Int64} <: Tuple{Integer}
    @test Tuple{Float64} <: Tuple{Real}
    @test Tuple{Float64} <: Tuple{Number}

    # Test 2: Tuple covariance with multiple elements
    @test Tuple{Int64, Float64} <: Tuple{Number, Number}
    @test Tuple{Int64, Int64} <: Tuple{Integer, Integer}
    @test Tuple{Int64, String} <: Tuple{Number, AbstractString}

    # Test 3: Array is INVARIANT
    # Vector{Int64} is NOT a subtype of Vector{Number}
    @test !(Vector{Int64} <: Vector{Number})
    @test !(Vector{Float64} <: Vector{Real})
    @test !(Array{Int64} <: Array{Number})

    # Test 4: Array variance - only exact matches are subtypes
    @test Vector{Int64} <: Vector{Int64}
    @test Vector{Float64} <: Vector{Float64}

    # Test 5: Tuple variance - reflexivity
    @test Tuple{Int64} <: Tuple{Int64}
    @test Tuple{String} <: Tuple{String}
end

# ===== source: types/tuple_core_subtype_gate_5615.jl =====

@testset "runtime Tuple subtype CoreType gate (Issue #5615)" begin
    @test Tuple{Int64,String} <: Tuple{Real,Any}
    @test !(Tuple{String} <: Tuple{Real})
    @test Tuple{Int64,Float64} <: Tuple{Integer,Real}
    @test !(Tuple{Int64,String} <: Tuple{Integer,Real})

    @test Tuple{} <: Tuple
    @test Tuple{Int64} <: Tuple
    @test !(Tuple{Int64} <: Tuple{String})
end

# ===== source: types/tuple_invariant_union_param_8582.jl =====
# Issue #8582: invariant positions nested inside Tuple covariance must compare
# a Union parameter as the whole type, not by membership in one union arm.


@testset "Tuple-wrapped invariant Union params (Issue #8582)" begin
    @test !(Tuple{Type{Int64}} <: Tuple{Type{Union{Nothing,Int64}}})
    @test !(Tuple{Vector{Int64}} <: Tuple{Vector{Union{Nothing,Int64}}})

    @test Tuple{Type{Union{Nothing,Int64}}} <: Tuple{Type{Union{Nothing,Int64}}}
    @test Tuple{Vector{Union{Nothing,Int64}}} <: Tuple{Vector{Union{Nothing,Int64}}}
end

# ===== source: types/tuple_vararg_subtype_5061.jl =====
# Issue #5061: `Tuple{Int, Vararg{T}}` and general-tuple intersection /
# subtype judgements. The fixed-prefix + trailing-`Vararg` normal form is
# compared element-by-element, with the trailing slots absorbed by the vararg
# element type. This table mirrors upstream `julia/src/subtype.c`
# (`subtype_tuple` / `subtype_tuple_varargs`).
#
# Notably this also covers the gap where bare `Tuple` (definitionally
# `Tuple{Vararg{Any}}`) was NOT recognised as a subtype of the universal
# vararg tuple `Tuple{Vararg{Any}}`.


@testset "fixed tuple <: trailing Vararg (Issue #5061)" begin
    @test Tuple{Int,Int} <: Tuple{Int,Vararg{Int}}
    @test Tuple{Int} <: Tuple{Int,Vararg{Int}}
    @test Tuple{Int,Int,Int} <: Tuple{Int,Vararg{Int}}
    @test Tuple{Int} <: Tuple{Vararg{Int}}
    @test Tuple{} <: Tuple{Vararg{Int}}
    @test Tuple{Int,Int} <: Tuple{Vararg{Int}}
    # element type widens under covariance
    @test Tuple{Int,Float64} <: Tuple{Int,Vararg{Real}}
    @test Tuple{Int,Int,Int,Int} <: Tuple{Int,Vararg{Integer}}
    @test Tuple{Int,Vararg{Int}} <: Tuple{Vararg{Integer}}
end

@testset "non-subtype tuple/Vararg cases (Issue #5061)" begin
    @test !(Tuple{Int,String} <: Tuple{Int,Vararg{Int}})
    @test !(Tuple{String} <: Tuple{Int,Vararg{Int}})
    @test !(Tuple{Int,Int,String} <: Tuple{Int,Vararg{Integer}})
    # a Vararg LHS may be empty, so it is not <: a fixed-arity tuple
    @test !(Tuple{Int,Vararg{Int}} <: Tuple{Int,Int})
    @test !(Tuple{Vararg{Int}} <: Tuple{Int,Vararg{Int}})
    # element type cannot narrow
    @test !(Tuple{Vararg{Real}} <: Tuple{Vararg{Int}})
    @test !(Tuple{Int,Vararg{Real}} <: Tuple{Int,Vararg{Int}})
    @test !(Tuple{Number,Vararg{Int}} <: Tuple{Real,Vararg{Int}})
end

@testset "Vararg{T,N} fixed-length tuples (Issue #5061)" begin
    @test Tuple{Int,Int,Int} <: Tuple{Vararg{Int,3}}
    @test !(Tuple{Int,Int} <: Tuple{Vararg{Int,3}})
    @test NTuple{3,Int} <: Tuple{Vararg{Int}}
end

@testset "bare Tuple === Tuple{Vararg{Any}} (Issue #5061)" begin
    # The bare `Tuple` datatype is the universal vararg tuple.
    @test Tuple <: Tuple{Vararg{Any}}
    @test Tuple{Vararg{Any}} <: Tuple
    @test Tuple{Int,Vararg{Int}} <: Tuple
    @test !(Tuple <: Tuple{Vararg{Int}})
    @test !(Tuple <: Tuple{Vararg{Real}})
    @test !(Tuple <: Tuple{Any})
    @test !(Tuple <: Tuple{Any,Vararg{Any}})
    @test !(Tuple <: Tuple{Int,Vararg{Int}})
end

@testset "tuple/Vararg typeintersect (Issue #5061)" begin
    @test typeintersect(Tuple{Int,Vararg{Int}}, Tuple{Vararg{Integer}}) ==
          Tuple{Int,Vararg{Int}}
    @test typeintersect(Tuple{Vararg{Int}}, Tuple{Int,Int}) == Tuple{Int,Int}
    @test typeintersect(Tuple{Int,Vararg{Real}}, Tuple{Vararg{Int}}) ==
          Tuple{Int,Vararg{Int}}
end

@testset "isa over Tuple{Int, Vararg{T}} (Issue #5061)" begin
    @test (1, 2, 3) isa Tuple{Int,Vararg{Int}}
    @test (1,) isa Tuple{Int,Vararg{Int}}
    @test !((1, "a") isa Tuple{Int,Vararg{Int}})
end

# ===== source: types/typeintersect_diagonal_unionall_5048.jl =====

@testset "diagonal UnionAll typeintersect narrows repeated TypeVar (Issue #5048)" begin
    diagonal = Tuple{T,T} where T<:Real

    @test typeintersect(diagonal, Tuple{Int64,Real}) === Tuple{Int64,Int64}
    @test typeintersect(Tuple{Int64,Real}, diagonal) === Tuple{Int64,Int64}
    @test typeintersect(diagonal, Tuple{Int64,Integer}) === Tuple{Int64,Int64}
    @test typeintersect(diagonal, Tuple{String,Real}) === Union{}

    @test string(typeintersect(diagonal, Tuple{Integer,Real})) ==
          "Tuple{T, T} where T<:Integer"
    @test string(typeintersect(diagonal, Tuple{Real,Real})) ==
          "Tuple{T, T} where T<:Real"
end

@testset "diagonal UnionAll typeintersect with invariant container occurrence (Issue #5048)" begin
    diagonal = Tuple{T,Vector{T}} where T<:Real

    @test typeintersect(diagonal, Tuple{Int64,Vector{Real}}) ===
          Tuple{Int64,Vector{Real}}
    @test typeintersect(diagonal, Tuple{Real,Vector{Int64}}) ===
          Tuple{Int64,Vector{Int64}}
    @test typeintersect(Tuple{Int64,Vector{Real}}, diagonal) ===
          Tuple{Int64,Vector{Real}}
    @test typeintersect(Tuple{Real,Vector{Int64}}, diagonal) ===
          Tuple{Int64,Vector{Int64}}

    @test typeintersect(diagonal, Tuple{String,Vector{Real}}) === Union{}
    @test typeintersect(diagonal, Tuple{Int64,Vector{String}}) === Union{}
    @test typeintersect(diagonal, Tuple{Integer,Vector{Float64}}) === Union{}
    @test typeintersect(diagonal, Tuple{Float64,Vector{Integer}}) === Union{}
end

# ===== source: types/typeintersect_invariant_5048.jl =====
# Concrete invariant-parametric `typeintersect` (Advances Issue #5048).
#
# Issue #5048 is the full set-theoretic `typeintersect` (TypeVar / UnionAll
# forall-exists). This fixture locks the CONCRETE invariant-parametric cases
# that are now correct after the invariant-subtype fix (Issue #5047 / #5563):
# when two parametric container types share a name but differ in an INVARIANT
# parameter (element type or array dimension), and neither is a subtype of the
# other, their intersection is the empty type `Union{}` — never a bare guess of
# one operand. Covariant cases (a true subtype relationship, union
# distribution, covariant Tuple element intersection) keep returning the
# non-empty intersection.
#
# All expectations below were verified against upstream Julia 1.12.
#
# Later #5564 and #5048 slices cover Dict/Set abstract-supertype parity,
# covariant Tuple element invariance, and diagonal UnionAll narrowing.


@testset "typeintersect: concrete invariant-parametric cases (Issue #5048)" begin
    # --- Invariant element parameter differs => Union{} (was wrongly the LHS) ---
    @test typeintersect(Vector{Int}, Vector{Real}) === Union{}
    @test typeintersect(Vector{Real}, Vector{Int}) === Union{}
    @test typeintersect(Vector{Int}, AbstractVector{Real}) === Union{}
    @test typeintersect(Vector{Float64}, AbstractVector{Int64}) === Union{}
    @test typeintersect(Matrix{Int}, Matrix{Float64}) === Union{}

    # --- Invariant array dimension differs => Union{} ---
    @test typeintersect(Vector{Int}, AbstractArray{Int,2}) === Union{}

    # --- Invariance is recursive through an invariant container element ---
    @test typeintersect(Vector{Vector{Int}}, Vector{Vector{Real}}) === Union{}

    # --- True subtype relationship => the narrower (subtype) operand stays ---
    @test typeintersect(Int, Real) === Int
    @test typeintersect(Vector{Int}, Vector) === Vector{Int}
    @test typeintersect(Vector{Int}, AbstractVector{Int}) === Vector{Int}
    @test typeintersect(Vector{Int}, AbstractArray{Int,1}) === Vector{Int}
    @test typeintersect(Matrix{Int}, AbstractArray{Int,2}) === Matrix{Int}

    # --- Union distribution still narrows to the intersecting member ---
    @test typeintersect(Union{Int,String}, Real) === Int

    # --- Covariant Tuple element intersection (each element intersected) ---
    @test typeintersect(Tuple{Int,String}, Tuple{Real,AbstractString}) ===
          Tuple{Int,String}

    # --- Disjoint operands => Union{} ---
    @test typeintersect(Int, String) === Union{}
    @test typeintersect(Tuple{Int,String}, Tuple{Real}) === Union{}

    # --- Property: typeintersect(A, B) <: A and <: B for the cases above ---
    @test typeintersect(Vector{Int}, AbstractVector{Int}) <: Vector{Int}
    @test typeintersect(Vector{Int}, AbstractVector{Int}) <: AbstractVector{Int}
    @test typeintersect(Union{Int,String}, Real) <: Union{Int,String}
    @test typeintersect(Union{Int,String}, Real) <: Real
end

# ===== source: types/typeintersect_nary_invariant_container_5048.jl =====
# Issue #5048: a diagonal-UnionAll tuple intersect must recover the diagonal
# variable from an invariant parametric container that holds it in ONE of
# several parameter slots — not only the single-parameter `Vector{T}` shape.
# `typeintersect(Tuple{T, Dict{Symbol,T}} where T<:Real, Tuple{Int64, Dict{Symbol,Real}})`
# previously collapsed to `Union{}` because the per-element invariant candidate
# helper handled only unary containers; it now generalizes to N-ary containers
# (`Dict{Symbol,T}`, `Dict{T,Symbol}`, `Pair{Symbol,T}`, `Dict{T,T}`), requiring
# every non-diagonal slot to be invariantly equal so a mismatched slot still
# yields `Union{}`.


@testset "N-ary invariant-container diagonal typeintersect (Issue #5048)" begin
    # the diagonal var in the value slot of a 2-param container
    @test typeintersect(Tuple{T,Dict{Symbol,T}} where T<:Real, Tuple{Int64,Dict{Symbol,Real}}) ==
          Tuple{Int64,Dict{Symbol,Real}}
    # the diagonal var in the key slot
    @test typeintersect(Tuple{T,Dict{T,Symbol}} where T<:Real, Tuple{Int64,Dict{Real,Symbol}}) ==
          Tuple{Int64,Dict{Real,Symbol}}
    # Pair value slot
    @test typeintersect(Tuple{T,Pair{Symbol,T}} where T<:Real, Tuple{Int64,Pair{Symbol,Real}}) ==
          Tuple{Int64,Pair{Symbol,Real}}
    # both parameter slots are the diagonal var (Dict{T,T}) — they must agree
    @test typeintersect(Tuple{T,Dict{T,T}} where T<:Real, Tuple{Int64,Dict{Real,Real}}) ==
          Tuple{Int64,Dict{Real,Real}}

    # the unary container shape still works (regression)
    @test typeintersect(Tuple{T,Vector{T}} where T<:Real, Tuple{Int64,Vector{Real}}) ==
          Tuple{Int64,Vector{Real}}

    # a non-diagonal slot that differs makes the whole intersection empty
    @test typeintersect(Tuple{T,Dict{Symbol,T}} where T<:Real, Tuple{Int64,Dict{Int,Real}}) ==
          Union{}
    # the two diagonal slots disagreeing makes it empty
    @test typeintersect(Tuple{T,Dict{T,T}} where T<:Real, Tuple{Int64,Dict{Int,Float64}}) ==
          Union{}
end

# ===== source: types/typeintersect_set_theoretic_5048.jl =====
# Issue #5048: set-theoretic `typeintersect` — the distributive law over Union,
# element-wise Tuple intersection (length mismatch -> Bottom), strict invariant
# parametric intersection, UnionAll x UnionAll / diagonal variables, plus the
# upstream correctness properties `typeintersect(A,B) <: A & <: B` and
# `A <: B => typeintersect(A,B) == A`. Shares the subtype engine env (#5615).


@testset "set-theoretic typeintersect (Issue #5048)" begin
    # distributive law over Union
    @test typeintersect(Union{Int,String}, Real) == Int
    @test typeintersect(Union{Int8,Int16,String}, Integer) == Union{Int8,Int16}
    @test typeintersect(Union{Int,String}, Union{String,Float64}) == String

    # element-wise Tuple, length mismatch -> Bottom
    @test typeintersect(Tuple{Int,Real}, Tuple{Integer,Float64}) == Tuple{Int,Float64}
    @test typeintersect(Tuple{Int,Int}, Tuple{Int}) == Union{}
    @test typeintersect(Tuple{Union{Int,Bool},String}, Tuple{Integer,String}) ==
          Tuple{Union{Int,Bool},String}

    # strict invariant parametric intersection
    @test typeintersect(Vector{Int}, Vector{Float64}) == Union{}
    @test typeintersect(Vector{Int}, Vector{Int}) == Vector{Int}
    @test typeintersect(Vector{Int}, AbstractVector{Int}) == Vector{Int}

    # UnionAll x concrete / UnionAll x UnionAll / diagonal variable.
    # For the UnionAll x UnionAll narrowing the result is the tighter bound; check
    # it by mutual subtyping (the built UnionAll is semantically `Vector{<:Integer}`).
    @test typeintersect(Vector, Vector{Int}) == Vector{Int}
    @test typeintersect(Vector{T} where T<:Real, Vector{T} where T<:Integer) <:
          (Vector{T} where T<:Integer)
    @test (Vector{T} where T<:Integer) <:
          typeintersect(Vector{T} where T<:Real, Vector{T} where T<:Integer)
    @test typeintersect(Tuple{T,T} where T, Tuple{Int,Real}) == Tuple{Int,Int}
    @test typeintersect(Tuple{T,T} where T, Tuple{Int,String}) == Union{}
    @test typeintersect(Dict{Int,V} where V, Dict{Int,String}) == Dict{Int,String}
    @test typeintersect(Tuple{Vector{T},T} where T, Tuple{Vector{Int},Int}) ==
          Tuple{Vector{Int},Int}

    # Type{...} and abstract intersections
    @test typeintersect(Type{Int}, DataType) == Type{Int}
    @test typeintersect(Real, Integer) == Integer
    @test typeintersect(Any, Int) == Int
    @test typeintersect(Int, String) == Union{}

    # correctness properties: I <: A and I <: B
    for (A, B) in [(Union{Int,String}, Real),
                   (Vector{T} where T<:Real, Vector{T} where T<:Integer),
                   (Tuple{T,T} where T, Tuple{Int,Real}),
                   (Pair{Int,T} where T, Pair{Int,String})]
        I = typeintersect(A, B)
        @test I <: A
        @test I <: B
    end

    # A <: B  =>  typeintersect(A,B) == A
    for (A, B) in [(Int, Real), (Vector{Int}, AbstractVector{Int}), (Int8, Integer)]
        @test typeintersect(A, B) == A
    end
end

# ===== source: types/typeintersect_unionall_abstract_container_5048.jl =====

# Issue #5048 (set-theoretic typeintersect, focused slice): a bare parametric
# container `UnionAll` met with a ground parametric instantiation. The
# concrete↔abstract container relation already worked when neither side was a
# `UnionAll` (`typeintersect(Vector{Int}, AbstractVector{Int}) == Vector{Int}`);
# the gap was a `where`-bound container on one side, which returned `Union{}`.
# Each `where` variable is forced (containers are invariant) to the matching
# positional parameter of the other operand, bound-checked, and the resulting
# concrete body verified `<:` the operand.

@testset "UnionAll ∩ abstract container (Issue #5048)" begin
    # Concrete container UnionAll ∩ abstract container (invariant element).
    @test typeintersect(Vector{T} where T<:Real, AbstractVector{Int}) === Vector{Int}
    @test typeintersect(Vector{T} where T, AbstractVector{Int}) === Vector{Int}
    @test typeintersect(Matrix{T} where T<:Real, AbstractMatrix{Int}) === Matrix{Int}
    @test typeintersect(Vector{T} where T<:Real, AbstractArray{Int,1}) === Vector{Int}

    # Symmetric operand order.
    @test typeintersect(AbstractVector{Int}, Vector{T} where T<:Real) === Vector{Int}

    # The element value flows from the abstract operand, respecting the bound.
    @test typeintersect(Vector{T} where T<:Real, AbstractVector{Float64}) === Vector{Float64}
    @test typeintersect(Vector{T} where T<:Real, AbstractVector{Bool}) === Vector{Bool}
    @test typeintersect(Vector{T} where T<:Integer, AbstractVector{Bool}) === Vector{Bool}

    # Multi-parameter container.
    @test typeintersect(Dict{K,V} where {K,V}, AbstractDict{Int,String}) === Dict{Int,String}

    # Partial instantiation: a fixed parameter is kept, the bound one flows in.
    @test typeintersect(Dict{Int,V} where V, AbstractDict{Int,String}) === Dict{Int,String}

    # Bound violation → empty.
    @test typeintersect(Vector{T} where T<:Real, AbstractVector{String}) === Union{}
    @test typeintersect(Vector{T} where T<:Integer, AbstractVector{Float64}) === Union{}

    # Unrelated family → empty (rejected by the subtype verification).
    @test typeintersect(Vector{T} where T<:Real, AbstractSet{Int}) === Union{}

    # Wrong dimensionality → empty.
    @test typeintersect(Vector{T} where T, AbstractArray{Int,2}) === Union{}

    # Diagonal variable across invariant positions must agree.
    @test typeintersect(Pair{T,T} where T, Pair{Int,Int}) === Pair{Int,Int}
    @test typeintersect(Pair{T,T} where T, Pair{Int,String}) === Union{}
    @test typeintersect(Pair{A,B} where {A,B}, Pair{Int,String}) === Pair{Int,String}
end

# ===== source: types/typeof_core_subtype_gate_5615.jl =====

@testset "runtime Type{T} subtype CoreType gate (Issue #5615)" begin
    @test Type{Int64} <: Type
    @test Type{Int64} <: Type{Int64}
    @test !(Type{Int64} <: Type{Integer})
    @test Type{Int64} <: Type{<:Integer}
    @test !(Type{String} <: Type{<:Integer})

    @test Type{Vector{Int64}} <: Type{<:AbstractVector}
    @test !(Type{Matrix{Int64}} <: Type{<:AbstractVector})
    @test Type{Matrix{Int64}} <: Type{<:AbstractMatrix}
end

# ===== source: types/types_anonymous_bounded_typevar_display_5644.jl =====

# Issue #5644: an ANONYMOUS bounded type variable — the internal placeholder name
# `_`, produced when parsing the covariant shorthand `Vector{<:Integer}` — must
# print with the bound-only shorthand `<:Bound` upstream, never echoing the `_`
# placeholder. sjulia rendered `Vector{_<:Integer}`. This is a display-only
# divergence (it does not affect type identity or `===`); the internal `_<:`
# spelling round-trips through parsing unchanged.

@testset "anonymous covariant bound prints as <:Bound, not _<:Bound (Issue #5644)" begin
    @test string(Vector{<:Integer}) == "Vector{<:Integer}"
    @test string(Set{<:Real}) == "Set{<:Real}"
    @test string(Type{<:Number}) == "Type{<:Number}"
    @test string(Array{<:Real,3}) == "Array{<:Real, 3}"
    @test string(Ref{<:Integer}) == "Ref{<:Integer}"

    # Multiple anonymous bounds in one type, and nested anonymous bounds.
    @test string(Dict{<:Integer,<:AbstractString}) == "Dict{<:Integer, <:AbstractString}"
    @test string(Vector{<:Vector{<:Real}}) == "Vector{<:Vector{<:Real}}"

    # A typeintersect result that carries an anonymous bound renders cleanly.
    @test string(typeintersect(Vector{Int}, Vector{<:Real})) == "Vector{Int64}"
end

@testset "named and unbounded type variables are unchanged (Issue #5644)" begin
    # A NAMED bounded typevar keeps its name; only the anonymous `_` is elided.
    @test string(Vector{T} where T<:Real) == "Vector{T} where T<:Real"
    # Plain concrete parametric types are unaffected.
    @test string(Vector{Int}) == "Vector{Int64}"
    @test string(Dict{String,Int}) == "Dict{String, Int64}"
end

# ===== source: types/types_array_family_subtype_direction_5640.jl =====

# Issue #5640: an abstract array-family type must NOT be reported as a subtype of
# a more concrete array-family type. The directional (abstract <-> concrete)
# relationship between array-family container names was previously ignored, so
# `AbstractVector{Int} <: Vector{Int}` wrongly returned `true`. This is also the
# direct cause of a `typeintersect` parity gap tracked under #5048.

@testset "array-family abstract is not subtype of concrete (Issue #5640)" begin
    # Wrong-direction relations: abstract/dense family is NOT a subtype of a
    # more concrete family.
    @test !(AbstractVector{Int} <: Vector{Int})
    @test !(AbstractVector{Int} <: Vector)
    @test !(DenseArray{Int} <: Array{Int})
    @test !(DenseVector{Int} <: Vector{Int})
    @test !(AbstractMatrix{Int} <: Matrix{Int})
    @test !(AbstractArray{Int} <: DenseArray{Int})
    @test !(AbstractArray{Int} <: Vector{Int})

    # Correct concrete -> abstract relations are preserved.
    @test Vector{Int} <: AbstractVector{Int}
    @test Vector{Int} <: AbstractArray{Int}
    @test Vector{Int} <: AbstractArray
    @test Array{Int} <: DenseArray{Int}
    @test Matrix{Int} <: AbstractMatrix{Int}
    @test DenseVector{Int} <: AbstractVector{Int}
    @test Vector{Int} <: AbstractVector

    # Invariance and rank constraints stay intact.
    @test !(Vector{Int} <: AbstractVector{Real})
    @test !(Vector{Int} <: AbstractMatrix{Int})
    @test !(Vector{Int} <: AbstractMatrix)
end

@testset "typeintersect picks concrete array side (Issue #5640 / #5048)" begin
    @test typeintersect(AbstractVector{Int}, Vector{Int}) === Vector{Int}
    @test typeintersect(Vector{Int}, AbstractVector{Int}) === Vector{Int}
    @test typeintersect(AbstractArray{Int,1}, Vector{Int}) === Vector{Int}
    @test typeintersect(DenseArray{Int}, Array{Int}) === Array{Int}
    @test typeintersect(AbstractMatrix{Int}, Matrix{Int}) === Matrix{Int}
    @test typeintersect(Vector{Int}, Vector{Real}) === Union{}
end

# ===== source: types/types_qualified_base_oneto_subtype_5874.jl =====
# Issue #5874: a qualified Base TYPE (e.g. `Base.OneTo`) used in a subtype
# expression must resolve to the type object, not be looked up as a Base function.
# Previously `Base.OneTo <: AbstractUnitRange` failed to compile with
# "Base has no function named OneTo".


@testset "qualified Base.OneTo in subtype expression (Issue #5874)" begin
    @test (Base.OneTo <: AbstractUnitRange) == true
    @test (Base.OneTo <: AbstractRange) == true

    # the qualified type object is a Type.
    @test Base.OneTo isa Type

    # other qualified Base type objects keep working in subtype position.
    @test (Base.RefValue <: Ref) == true
end

# ===== source: types/types_typeof_datatype_subtype_5048.jl =====

# Issue #5048 (set-theoretic typeintersect): a `Type{T}` is a subtype of
# `DataType` exactly when `T` is itself a nominal DataType — a concrete or
# abstract type, or a fully-applied parametric type — but NOT when `T` is a
# `Union`, a bare parametric (a `UnionAll`), or a `Type{<:Bound}` (also a
# `UnionAll`). sjulia previously reported every `Type{T} <: DataType` as `false`,
# so `typeintersect(Type{Int}, DataType)` collapsed to `Union{}` instead of the
# concrete `Type{Int}` side.

@testset "Type{T} <: DataType honors T's nominal-DataType shape (Issue #5048)" begin
    # T is a nominal DataType -> Type{T} <: DataType.
    @test Type{Int} <: DataType
    @test Type{Integer} <: DataType
    @test Type{String} <: DataType
    @test Type{Any} <: DataType
    @test Type{Vector{Int}} <: DataType
    @test Type{Vector{Real}} <: DataType

    # T is a Union / bare parametric (UnionAll) / Type{<:Bound} -> not a DataType.
    @test !(Type{Union{Int,Bool}} <: DataType)
    @test !(Type{Vector} <: DataType)
    @test !(Type{<:Real} <: DataType)

    # The reverse never holds.
    @test !(DataType <: Type{Int})

    # `Type{T} <: Type` stays true regardless.
    @test Type{Int} <: Type
    @test Type{<:Real} <: Type
end

@testset "typeintersect(Type{T}, DataType) keeps the concrete Type side (Issue #5048)" begin
    @test typeintersect(Type{Int}, DataType) === Type{Int}
    @test typeintersect(DataType, Type{Int}) === Type{Int}
    @test typeintersect(Type{Float64}, DataType) === Type{Float64}
    @test typeintersect(Type{Integer}, DataType) === Type{Integer}
    @test typeintersect(Type{String}, DataType) === Type{String}
    @test typeintersect(Type{Vector{Int}}, DataType) === Type{Vector{Int}}
    @test typeintersect(Type{Vector{Real}}, DataType) === Type{Vector{Real}}

    # Non-DataType T intersects DataType to Union{}.
    @test typeintersect(Type{Union{Int,Bool}}, DataType) === Union{}
    @test typeintersect(Type{Vector}, DataType) === Union{}
end

# ===== source: types/union_normalization_5066.jl =====
# Issue #5066: deep nested Union normalization (flatten / dedup / sort / collapse)
# Equal Unions share one canonical normal form, so `===` is independent of
# nesting depth, member order, and duplicates — matching upstream Julia's
# `jl_type_union` (julia/src/jltypes.c).


@testset "Union normalization: flatten / dedup / sort / collapse (Issue #5066)" begin
    # Flatten nested unions
    @test Union{Int, Union{Float64, Int}} === Union{Int, Float64}
    @test Union{Int, Union{Float64, String}} === Union{Int, Float64, String}
    @test Union{Union{Int, Float64}, Union{String, Char}} === Union{Int, Float64, String, Char}

    # Order-independent identity (canonical sort)
    @test Union{Int, Float64} === Union{Float64, Int}
    @test Union{String, Int, Float64} === Union{Float64, Int, String}

    # Singleton collapse: a one-element union is the element itself
    @test Union{Int} === Int
    @test Union{String} === String

    # Bottom (empty union)
    @test Union{} === Union{}
    @test Union{Union{}, Int} === Int

    # Duplicate removal
    @test Union{Int, Int} === Int
    @test Union{Int, Float64, Int} === Union{Int, Float64}

    # Subtype absorption (A <: B removes A)
    @test Union{Int, Integer} === Integer
    @test Union{Int8, Int16, Integer} === Integer
    @test Union{Int, Real, Float64} === Real
    @test Union{Int, Any} === Any

    # Nested + duplicate + reorder all at once
    @test Union{String, Union{Int, String}, Int} === Union{Int, String}
end

@testset "Union canonical display order (Issue #5066)" begin
    # singleton < isbits < other DataType < non-DataType; ties break by name
    @test string(Union{Int, Float64}) == "Union{Float64, Int64}"
    @test string(Union{String, Int}) == "Union{Int64, String}"
    @test string(Union{Nothing, Int, Missing}) == "Union{Missing, Nothing, Int64}"
    @test string(Union{Char, String, Symbol}) == "Union{Char, String, Symbol}"
    @test string(Union{Int128, Int16, Int32, Int64, Int8}) ==
          "Union{Int128, Int16, Int32, Int64, Int8}"
end

# ===== source: types/union_subtype.jl =====
# Test Union subtype checking: Int <: Union{Int, Float64}
# Verifies that subtype operator works with Union types


@testset "Union type subtype checking: T <: Union{A, B}" begin

    result = 0.0

    # Basic subtype checks: T <: Union{A, B} iff T <: A or T <: B
    # Note: Must assign to variable before using in if condition
    test1 = Int <: Union{Int, Float64}
    if test1 == 1  # Using == 1 since <: returns 0/1
        result = result + 1.0  # Should be true
    end

    test2 = Float64 <: Union{Int, Float64}
    if test2 == 1
        result = result + 1.0  # Should be true
    end

    # String is NOT a subtype of Union{Int, Float64}
    test3 = String <: Union{Int, Float64}
    if test3 == 0
        result = result + 1.0  # Should be false (0)
    end

    # Abstract type in union: Int <: Number, so Int <: Union{Number, String}
    test4 = Int <: Union{Number, String}
    if test4 == 1
        result = result + 1.0  # Int <: Number, so true
    end

    # Union subtype of supertype: Union{A, B} <: T iff A <: T and B <: T
    test5 = Union{Int, Float64} <: Number
    if test5 == 1
        result = result + 1.0  # Both Int <: Number and Float64 <: Number
    end

    # Union{Int, String} is NOT a subtype of Number (String <: Number is false)
    test6 = Union{Int, String} <: Number
    if test6 == 0
        result = result + 1.0
    end

    @test (result) == 6.0
end

# ===== source: types/unionall_apply_parenthesized_8430.jl =====

@testset "parenthesized UnionAll application (Issue #8430)" begin
    applied = (Vector{T} where T){Int}
    @test applied === Vector{Int}
    @test applied == Vector{Int}
    @test typeof(applied) === DataType
    @test string(applied) == "Vector{Int64}"
end

# ===== source: types/vararg_len_subtype_intersect_5062.jl =====
# Issue #5062: subtype / typeintersect involving the fixed-length value
# parameter `N` of `Vararg{T,N}` (and the synonymous `NTuple{N,T}`).
#
# Before the fix, sjulia handled the *pattern* side of the alias
# (`Tuple{Int,Int,Int} <: NTuple{3,Int}`) but not the *actual* side, so the
# reverse relation `NTuple{3,Int} <: Tuple{Int,Int,Int}` and the equivalence
# direction were rejected, and `typeintersect` did not flatten the alias.
#
# Fix: expand a concrete-length `Vararg{T,N}` element into the flat
# `Tuple{T, ..., T}` shape on both operands during subtype checking and
# intersection, matching upstream Julia's identity
# `Tuple{Vararg{T,N}} === Tuple{T, ..., T}`.


@testset "NTuple{N,T} <-> Tuple flat form subtyping (Issue #5062)" begin
    # Both directions of the equivalence hold.
    @test NTuple{3,Int} <: Tuple{Int,Int,Int}
    @test Tuple{Int,Int,Int} <: NTuple{3,Int}
    # Length mismatch is rejected in both directions.
    @test !(NTuple{2,Int} <: Tuple{Int,Int,Int})
    @test !(Tuple{Int,Int} <: NTuple{3,Int})
    # Element covariance survives the alias expansion.
    @test NTuple{3,Int} <: Tuple{Real,Real,Real}
    @test !(NTuple{3,Real} <: Tuple{Int,Int,Int})
end

@testset "Vararg{T,N} concrete length subtyping (Issue #5062)" begin
    @test Tuple{Vararg{Int,3}} <: Tuple{Int,Int,Int}
    @test !(Tuple{Vararg{Int,3}} <: Tuple{Int,Int})
    # Two fixed-length varargs: equal length + covariant element.
    @test NTuple{3,Int} <: NTuple{3,Integer}
    @test !(NTuple{3,Int} <: NTuple{2,Int})
end

@testset "typeintersect over the fixed-length vararg alias (Issue #5062)" begin
    @test typeintersect(NTuple{3,Int}, Tuple{Int,Int,Int}) === Tuple{Int,Int,Int}
    @test typeintersect(NTuple{2,Int}, Tuple{Int,Int,Int}) === Union{}
    @test typeintersect(Tuple{Vararg{Int,3}}, Tuple{Int,Int,Int}) === Tuple{Int,Int,Int}
end

# ===== source: types/where_bare_typevar_body_5570.jl =====

@testset "bare typevar where body collapses to bound (Issue #5570)" begin
    @test (T where T) === Any
    @test string(T where T) == "Any"
    @test (T where T<:Real) === Real
    @test string(T where T<:Real) == "Real"
    @test Int <: (T where T)
    @test !(String <: (T where T<:Real))
    @test typeof(T where T) === DataType
    @test typeof(T where T<:Real) === DataType
end

# ===== source: types/where_subtype_existsright_5047.jl =====
# Exists-right subtype solving: `A <: (B where V...)` where the RHS is a
# UnionAll (Advances Issue #5047, also #5049).
#
# Decides `A <: UnionAll` by finding bindings for the bound var(s) that make
# `A <: B[bindings]`, respecting each var's bounds and the DIAGONAL rule (a var
# appearing in multiple covariant slots must take ONE consistent value).
#
# This builds on the #5569 increment, which lowered a value-position `where`
# expression to a first-class UnionAll value. Previously the runtime `<:` on
# such a value ignored the `where` clause entirely (treated bound vars as `Any`,
# enforcing neither bounds nor the diagonal rule), so e.g.
# `Tuple{Int,String} <: (Tuple{T,T} where T)` wrongly returned `true`.
#
# OUT OF SCOPE (later increments): LHS-UnionAll (forall-left) and full
# forall-exists alternation (both sides UnionAll, #5049). The degenerate
# bare-typevar-BODY `where` collapse (`T where T === Any`) is covered by the
# focused Issue #5570 fixture.
#
# All expectations below were verified against upstream Julia 1.12.


@testset "exists-right: diagonal rule (Issue #5047)" begin
    # Same var T in two covariant tuple slots must take one consistent value.
    @test (Tuple{Int,Int} <: (Tuple{T,T} where T)) == true   # T=Int
    @test (Tuple{Int,String} <: (Tuple{T,T} where T)) == false # diagonal: T cannot be both
    @test (Tuple{Int,Real} <: (Tuple{T,T} where T)) == false   # Int != Real
    # Distinct vars T,S can take independent values.
    @test (Tuple{Int,Float64} <: (Tuple{T,S} where {T,S})) == true
end

@testset "exists-right: bounds (Issue #5047)" begin
    @test (Vector{Int} <: (Vector{T} where T)) == true
    @test (Vector{Int} <: (Vector{T} where T<:Real)) == true   # Int <: Real
    @test (Vector{String} <: (Vector{T} where T<:Real)) == false
    @test (Tuple{Int,Int} <: (Tuple{T,T} where T<:Integer)) == true
    @test (Tuple{Int,Int} <: (Tuple{T,T} where T<:AbstractString)) == false
end

@testset "exists-right: container shapes (Issue #5047)" begin
    @test (Dict{String,Int} <: (Dict{K,V} where {K,V})) == true
    @test (Tuple{Int,Int,Int} <: (Tuple{Vararg{T}} where T)) == true
end

# NOTE: the degenerate bare-typevar-BODY `where` cases — `Int <: (T where T)`
# (true upstream, since `T where T === Any`) and `String <: (T where T<:Real)`
# (false upstream, since `T where T<:Real === Real`) — are covered by the
# focused Issue #5570 fixture. This fixture stays scoped to exists-right
# UnionAll solver behavior.

# --- MUST STAY CORRECT: non-`where` subtyping, incl. invariant cases. ---
@testset "non-where subtyping regression guard (Issue #5047)" begin
    @test (Vector{Int} <: Vector{Real}) == false
    @test (Dict{String,Int} <: AbstractDict{String,Int}) == true
    @test (Tuple{Int} <: Tuple{Real}) == true
    @test (Vector{Int} <: AbstractVector{Int}) == true
end

# ===== source: types/where_subtype_forallleft_5047.jl =====
# Forall-left subtype solving: `(B where V...) <: C` where the LHS is a
# UnionAll (Advances Issue #5047, also #5049).
#
# Decides `(B where V...) <: C` by introducing a fresh RIGID variable for each
# bound var (constrained by its declared bounds) and checking `B[rigid] <: C`
# holds for ALL such rigid choices — i.e. the bound var behaves as an opaque
# type confined to its bounds. Combined with the already-merged exists-right
# solver (#5571), the rigid LHS var flowing into a RHS UnionAll pattern yields
# forall-exists ALTERNATION for the common single/diagonal-var cases, e.g.
# `(Tuple{T} where T<:Integer) <: (Tuple{S} where S<:Real)` (∀T<:Integer there
# exists S<:Real, namely S=T).
#
# Previously the LHS `where` clause was dropped: the body's bound var was parsed
# as an UNBOUNDED typevar, so its declared upper bound never flowed into the
# subtype check and alternation cases wrongly returned `false`.
#
# All expectations below were verified against upstream Julia 1.12.


@testset "forall-left: bare bounded var (Issue #5047)" begin
    @test ((Vector{T} where T) <: AbstractVector) == true       # ∀T: Vector{T}<:AbstractVector
    @test ((Vector{T} where T) <: Vector{Int}) == false
    @test ((Vector{T} where T<:Real) <: AbstractVector) == true
    @test ((Vector{T} where T<:Integer) <: AbstractVector) == true
    @test ((Tuple{T,T} where T) <: Tuple) == true
    @test ((Tuple{T} where T) <: Tuple{Int}) == false          # ∃ a T (e.g. String) breaking it
end

@testset "forall-left: builtin UnionAll aliases (Issue #5047)" begin
    @test (Array <: AbstractArray) == true                      # Array is a UnionAll on the left
    # NOTE: `Vector <: AbstractVector` (true upstream) is NOT exercised here: in
    # this VM bare `Vector` renders/routes as the rank-erased `Array` (string
    # "Array", no rank-1 marker, no `where`), so it never reaches the UnionAll
    # subtype arm — it is a separate name-rendering quirk, out of scope for the
    # forall-left engine increment.
end

@testset "forall-left + exists-right ALTERNATION (Issue #5047/#5049)" begin
    # Representative Issue #5049 shape: both sides carry type variables.
    # ∀T ∃S,U: Tuple{T,T} <: Tuple{S,U} holds by S=T, U=T.
    @test ((Tuple{T,T} where T) <: (Tuple{S,U} where {S,U})) == true
    # Reverse direction fails: not every Tuple{S,U} has equal element types.
    @test ((Tuple{S,U} where {S,U}) <: (Tuple{T,T} where T)) == false
    # ∀T<:Integer ∃S<:Real (S=T): Tuple{T}<:Tuple{S} holds.
    @test ((Tuple{T} where T<:Integer) <: (Tuple{S} where S<:Real)) == true
    # ∀T<:Real: NOT every T admits an S<:Integer with Tuple{T}<:Tuple{S}.
    @test ((Tuple{T} where T<:Real) <: (Tuple{S} where S<:Integer)) == false
    # Invariant element under alternation: S:=T (T<:Integer<:Real) works.
    @test ((Vector{T} where T<:Integer) <: (Vector{S} where S<:Real)) == true
    # Diagonal both sides: T=T forces S=S; S:=T satisfies S<:Real.
    @test ((Tuple{T,T} where T<:Integer) <: (Tuple{S,S} where S<:Real)) == true
end

# --- MUST STAY CORRECT: exists-right (#5571), invariant, and non-where. ---
@testset "regression guard: exists-right + invariant (Issue #5047)" begin
    # Exists-right diagonal/bounds (#5571) must still hold.
    @test (Tuple{Int,Int} <: (Tuple{T,T} where T)) == true
    @test (Tuple{Int,String} <: (Tuple{T,T} where T)) == false
    @test (Vector{Int} <: (Vector{T} where T<:Real)) == true
    @test (Vector{String} <: (Vector{T} where T<:Real)) == false
    # Invariant + plain subtyping.
    @test (Vector{Int} <: Vector{Real}) == false
    @test (Dict{String,Int} <: AbstractDict{String,Int}) == true
    @test (Tuple{Int} <: Tuple{Real}) == true
    @test (Vector{Int} <: AbstractVector{Int}) == true
end

# ===== source: types/wrapper_array_core_subtype_gate_5615.jl =====

@testset "wrapper array subtype CoreType gate (Issue #5615)" begin
    v = view([1, 2, 3], 1:2)
    vt = typeof(v)

    @test vt <: AbstractVector{Int64}
    @test vt <: AbstractArray{Int64,1}
    @test !(vt <: DenseVector{Int64})
    @test !(vt <: DenseArray{Int64,1})

    r = reshape(view([1, 2, 3, 4], 1:4), 2, 2)
    rt = typeof(r)

    @test rt <: AbstractMatrix{Int64}
    @test rt <: AbstractArray{Int64,2}
    @test !(rt <: DenseMatrix{Int64})
    @test !(rt <: DenseArray{Int64,2})
end

true
