# Issue #7266: a single array-family argument whose element type is unknown at
# compile time — most notably a comprehension `[expr for ...]`, which the
# compiler images as the bare `JuliaType::Struct("Vector")` — loose-matched a
# `::Integer` (abstract-scalar) method. The struct-parents dispatch fallback
# (`struct_is_subtype_of_abstract`) "conservatively accepted" the built-in
# `Vector` family (it is not in the user-declared hierarchy) as a subtype of any
# abstract bound, so `Vector <: Integer` wrongly held and a comprehension
# argument routed to the `::Integer` constructor/method.
#
# Symptom was context-dependent only incidentally: the failing call lived inside
# another constructor (`Foo(k::Integer) = Foo([1.0/k for _ in 1:k])`), where the
# inner comprehension is the only inline-comprehension call site; the identical
# top-level call used an array literal (the precise `Vector{Float64}` image),
# which dispatched correctly. The real fault is the comprehension's bare-`Vector`
# image mis-matching `::Integer`.
#
# Fix: a built-in struct family (`Vector`/`Matrix`/`Array`/`Dict`/...) is walked
# through its known built-in supertype chain instead of being conservatively
# accepted, so it never reports `<: Integer`; and a single rank-unknown array
# argument that matches no method statically routes to runtime dispatch (the
# concrete `Vector{Float64}` value selects the right method), matching upstream
# Julia 1.12. Verified against upstream `julia`.

using Test

# The exact #7266 reproduction: an `::Integer` constructor whose body builds a
# vector inline and re-dispatches through the `::AbstractVector{<:Real}` ctor.
struct Foo
    p
    k
end
Foo(p::AbstractVector{<:Real}) = Foo(p, length(p))
Foo(k::Integer) = Foo([1.0 / k for _ in 1:k])

# `Foo(3)` (inside the ::Integer ctor) and `Foo([...])` (top level) must agree.
ok_from_integer_ctor() = Foo(3).p == [1.0 / 3, 1.0 / 3, 1.0 / 3] && Foo(3).k == 3
ok_from_vector_literal() =
    Foo([0.25, 0.25, 0.25]).p == [0.25, 0.25, 0.25] && Foo([0.25, 0.25, 0.25]).k == 3
# A comprehension passed directly at top level dispatches to the AbstractVector
# method, not the Integer one.
ok_top_level_comprehension() = Foo([1.0 / 3 for _ in 1:3]).k == 3

# Generalize beyond constructors: a plain function with `::Integer` and
# `::AbstractVector{<:Real}` overloads called with a comprehension.
g(p::AbstractVector{<:Real}) = :abstractvector
g(k::Integer) = :integer
ok_func_comprehension() = g([1.0 / 3 for _ in 1:3]) == :abstractvector
ok_func_literal_array() = g([1.0, 2.0, 3.0]) == :abstractvector
ok_func_integer() = g(5) == :integer

# Matrix comprehension routes to ::AbstractMatrix, not ::Integer.
h(m::AbstractMatrix) = :matrix
h(k::Integer) = :integer
ok_matrix_comprehension() = h([i + j for i in 1:2, j in 1:2]) == :matrix

# A comprehension argument with ONLY an ::Integer method must MethodError (it is
# NOT a subtype of Integer), exactly like upstream Julia.
intonly(k::Integer) = :int
ok_no_loose_integer_match() =
    try
        intonly([1.0 / 3 for _ in 1:3])
        false
    catch e
        isa(e, MethodError)
    end

@testset "comprehension/array-family arg never loose-matches ::Integer (#7266)" begin
    @test ok_from_integer_ctor()
    @test ok_from_vector_literal()
    @test ok_top_level_comprehension()
    @test ok_func_comprehension()
    @test ok_func_literal_array()
    @test ok_func_integer()
    @test ok_matrix_comprehension()
    @test ok_no_loose_integer_match()
end

# Final value gates the in-harness nextest run on correctness, not just no-throw.
ok_from_integer_ctor() &&
    ok_from_vector_literal() &&
    ok_top_level_comprehension() &&
    ok_func_comprehension() &&
    ok_func_literal_array() &&
    ok_func_integer() &&
    ok_matrix_comprehension() &&
    ok_no_loose_integer_match()
