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

true
