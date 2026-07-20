# LinRange carries the length type parameter L, matching upstream's
# struct LinRange{T,L<:Integer}: typeof shows both parameters, partial
# application LinRange{T} still matches values and stays <: AbstractRange,
# and the trait pipeline (IteratorSize/eltype) dispatches on the two-parameter
# form (Issues #11441, #11449).
using Test

@testset "LinRange{T,L} type parameters (Issue #11441)" begin
    r = LinRange(0.0, 1.0, 5)
    @test typeof(r) == LinRange{Float64, Int64}
    @test r isa LinRange{Float64}
    @test r isa LinRange

    # range(start, stop; length) falls back to LinRange for Big endpoints
    rb = range(big(1), big(2), length=3)
    @test typeof(rb) == LinRange{BigFloat, Int64}
    @test collect(rb) == [big(1.0), big(1.5), big(2.0)]

    # Rational endpoints exercise the non-float LinRange lane
    rr = LinRange(1//2, 3//2, 3)
    @test collect(rr) == [0.5, 1.0, 1.5]
end

@testset "LinRange partial application and subtyping (Issue #11441)" begin
    @test LinRange{Float64} <: AbstractRange
    @test LinRange{Float64, Int64} <: AbstractRange
    @test LinRange <: AbstractRange
end

@testset "LinRange trait pipeline on two-parameter form (Issue #11449)" begin
    r = LinRange(0.0, 1.0, 5)
    @test Base.IteratorSize(r) == Base.HasShape{1}()
    @test Base.IteratorSize(typeof(r)) == Base.HasShape{1}()
    @test eltype(r) == Float64
    @test eltype(typeof(r)) == Float64
    @test repr(r) == "LinRange{Float64}(0.0, 1.0, 5)"
    @test step(r) == 0.25
    @test r[3] == 0.5
    @test length(r) == 5
    @test first(r) == 0.0 && last(r) == 1.0
    @test size(r) == (5,)
end

true
