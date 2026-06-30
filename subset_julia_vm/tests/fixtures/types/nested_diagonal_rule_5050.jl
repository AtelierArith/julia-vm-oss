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

true
