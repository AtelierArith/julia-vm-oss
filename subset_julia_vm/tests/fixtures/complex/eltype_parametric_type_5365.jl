# Issue #5365: `eltype(Complex{T})` returned the inner parameter `T` instead of
# the type itself (`Complex{T}`). Upstream Julia defines
# `eltype(::Type{T}) where {T<:Number} = T`, so a scalar Number type is its own
# element type (`eltype(Complex{Float64}) === Complex{Float64}`).
#
# Root cause: at compile time, a parametric type literal `eltype(Complex{Float64})`
# (arg inferred as `Type{Complex{Float64}}`) wrongly dispatched to
# `eltype(::Type{<:Tuple})`. The struct->parent ancestry fallback
# (`struct_is_subtype_of_abstract`) reported `Complex{Float64} <: Tuple` as true
# because (a) it looked the *parametric* name up in a base-name-keyed map and
# missed, then "conservatively accepted", and (b) the chain walk followed the
# `Any -> Any` self-edge until its cycle guard also conservatively accepted. So
# ANY `Type{Struct}` matched the Tuple method, whose body joins `.parameters`
# and thus returned the first type parameter (`Float64`). The fix rejects a
# concrete built-in bound (`Tuple`/`Pairs{...}`/...) outright, strips type
# parameters before the lookup, and stops the walk at `Any`.

using Test

@testset "eltype of parametric Number type literals (Issue #5365)" begin
    # Complex (the original report): a scalar Number type is its own eltype.
    @test eltype(Complex{Float64}) == Complex{Float64}
    @test eltype(Complex{Int64}) == Complex{Int64}
    @test eltype(Complex{Float32}) == Complex{Float32}
    @test eltype(Complex) == Complex

    # Other Number parametric types behave the same.
    @test eltype(Rational{Int64}) == Rational{Int64}
    @test eltype(Rational{Int32}) == Rational{Int32}

    # Regression guards: scalar/concrete and genuine container element types.
    @test eltype(Float64) == Float64
    @test eltype(Int64) == Int64
    @test eltype(Vector{Int64}) == Int64
    @test eltype(Vector{Complex{Float64}}) == Complex{Float64}
    @test eltype(Matrix{Float64}) == Float64

    # Value form still works (these already passed; keep them green).
    @test eltype(1.0 + 2.0im) == Complex{Float64}
    @test eltype(1 // 2) == Rational{Int64}

    # Via a variable slot (the runtime path, which was already correct).
    T = Complex{Float64}
    @test eltype(T) == Complex{Float64}
end

true  # Test passed
