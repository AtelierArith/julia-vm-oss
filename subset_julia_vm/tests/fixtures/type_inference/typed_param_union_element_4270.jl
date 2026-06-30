using Test

# Issue #5595 (advances #4270): a function parameter ANNOTATED with a `Vector`
# whose element type is a `Union` previously lost the element union and inferred
# `a[1]` as `Any`. Concrete element types were unaffected, and the same
# signature given without an annotation (only via the call `Tuple`) inferred
# correctly — so this was specific to the parameter-annotation -> lattice
# conversion (`julia_type_to_concrete_or_any` collapsing `LatticeType::Union`
# to `ConcreteType::Any` instead of `ConcreteType::UnionOf`).
#
# Values verified field-for-field against upstream Julia 1.12.6.

# Concrete element types are unaffected (regression guard).
elt_int(a::Vector{Int64}) = a[1]
elt_float(a::Vector{Float64}) = a[1]

# Union element types must be preserved through the annotation.
elt_union_in(a::Vector{Union{Int64,Nothing}}) = a[1]
elt_union_is(a::Vector{Union{Int64,String}}) = a[1]
elt_union_if(a::Vector{Union{Int64,Float64}}) = a[1]

# Guarded forms (the #4270 index-narrowing shape): a mutable array element read
# is not narrowed, so the declared element union flows to the result, joined
# with the fallback literal.
guard_tern(a::Vector{Union{Int64,Nothing}}) = a[1] !== nothing ? a[1] : 0
function guard_if(a::Vector{Union{Int64,Nothing}})
    if a[1] !== nothing
        return a[1]
    end
    return 0
end

@testset "typed concrete-element params unaffected (#5595)" begin
    @test Base.infer_return_type(elt_int, Tuple{Vector{Int64}}) === Int64
    @test Base.infer_return_type(elt_float, Tuple{Vector{Float64}}) === Float64
end

@testset "typed Union-element params preserve element union (#5595)" begin
    @test Base.infer_return_type(elt_union_in, Tuple{Vector{Union{Int64,Nothing}}}) ==
        Union{Nothing,Int64}
    @test Base.infer_return_type(elt_union_is, Tuple{Vector{Union{Int64,String}}}) ==
        Union{Int64,String}
    @test Base.infer_return_type(elt_union_if, Tuple{Vector{Union{Int64,Float64}}}) ==
        Union{Float64,Int64}
end

@testset "typed Union-element guarded index narrowing (#4270/#5595)" begin
    @test Base.infer_return_type(guard_tern, Tuple{Vector{Union{Int64,Nothing}}}) ==
        Union{Nothing,Int64}
    @test Base.infer_return_type(guard_if, Tuple{Vector{Union{Int64,Nothing}}}) ==
        Union{Nothing,Int64}
    # Runtime behavior matches the inferred shape (use a typed constructor so the
    # element union is explicit, independent of mixed-literal element inference).
    @test guard_tern(Union{Int64,Nothing}[7, nothing]) == 7
    @test guard_tern(Union{Int64,Nothing}[nothing, 7]) == 0
end

true
