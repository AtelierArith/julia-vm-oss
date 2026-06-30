using Test

# Issue #7334: a `::AbstractMatrix` (= `AbstractArray{T,2}`) method parameter must
# only match 2-dimensional array values. Before the fix, the compile-time
# struct-parents fallback conservatively accepted a *function singleton*
# (`typeof(sin)`, a `Function`) as a subtype of `AbstractMatrix` — so `h(sin)`
# loose-matched `h(::AbstractMatrix)` and even won dispatch over the specific
# `h(::Function)` method (the same conservative-accept class as #7266). Upstream
# Julia selects `h(::Function)` for `h(sin)` and raises a `MethodError` when only
# an `::AbstractMatrix` method exists. This blocked the #7275 Interact sample
# (`scatter(sin)` crashed inside `scatter(::AbstractMatrix)`).
@testset "Issue #7334: ::AbstractMatrix does not loose-match a Function" begin
    h(m::AbstractMatrix) = "matrix"
    h(x::Function) = "function"
    h(x::Int) = "int"
    h(x::AbstractString) = "string"

    # A Function argument must reach the specific `::Function` method, not
    # `::AbstractMatrix`.
    @test h(sin) == "function"
    @test h(cos) == "function"

    # The other specific methods are unaffected.
    @test h(3) == "int"
    @test h("hi") == "string"

    # A genuine 2-D array still reaches `::AbstractMatrix`.
    @test h([1.0 2.0; 3.0 4.0]) == "matrix"

    # With only an `::AbstractMatrix` method (no `::Function` competitor), a
    # Function argument has NO matching method and must raise a MethodError —
    # the conservative accept must not silently route it into the matrix method.
    g(m::AbstractMatrix) = "g-matrix"
    g(x::Int) = "g-int"
    @test_throws MethodError g(sin)
    @test g([1 2; 3 4]) == "g-matrix"
    @test g(5) == "g-int"
end

true
