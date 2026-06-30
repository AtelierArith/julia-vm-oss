using Test

# Issue #8025: a parameterized matrix method `::AbstractMatrix{<:T}` (and the
# concrete `::Matrix{T}` spelling) must be ranked more specific than the bare
# `::AbstractMatrix` when dispatching a `Matrix{T}` whose element type `T` is a
# USER struct. Before the fix the runtime dispatch type of a user-struct-element
# array collapsed its element type to `Any` (the registry-free array-wrapper
# type reported `Matrix{Any}`), so the parametric `::AbstractMatrix{<:MyNum}`
# method failed to match against its bare `::AbstractMatrix` sibling and dispatch
# fell to the less-specific `"generic"` method. Upstream Julia selects the
# parameterized method. Built-in element types (`Real`/`Float64`) already worked
# because their array element type was tracked precisely.

struct MyNum <: Number
    v::Int
end

# Bare AbstractMatrix vs parameterized AbstractMatrix{<:MyNum}.
h(M::AbstractMatrix) = "generic"
h(M::AbstractMatrix{<:MyNum}) = "specific-Num"

# Definition order must not change the winner.
h2(M::AbstractMatrix{<:MyNum}) = "specific-Num"
h2(M::AbstractMatrix) = "generic"

# The concrete `Matrix{MyNum}` spelling is also more specific than bare
# `AbstractMatrix`.
h3(M::AbstractMatrix) = "generic"
h3(M::Matrix{MyNum}) = "specific-Num"

# The explicit `where {T<:MyNum}` spelling keeps working.
h4(M::AbstractMatrix) = "generic"
h4(M::AbstractMatrix{T}) where {T<:MyNum} = "specific-Num"

# Built-in element bounds keep their (already correct) behavior.
g(M::AbstractMatrix) = "generic"
g(M::AbstractMatrix{<:Real}) = "specific-Real"

@testset "Issue #8025: parametric AbstractMatrix{<:Num} beats bare AbstractMatrix" begin
    A = [MyNum(1) MyNum(2); MyNum(3) MyNum(4)]   # typeof: Matrix{MyNum}
    @test typeof(A) == Matrix{MyNum}
    @test eltype(A) == MyNum

    @test h(A) == "specific-Num"
    @test h([1.0 2.0; 3.0 4.0]) == "generic"

    @test h2(A) == "specific-Num"

    @test h3(A) == "specific-Num"
    @test h3([1 2; 3 4]) == "generic"

    @test h4(A) == "specific-Num"

    @test g([1.0 2.0; 3.0 4.0]) == "specific-Real"
end

true
