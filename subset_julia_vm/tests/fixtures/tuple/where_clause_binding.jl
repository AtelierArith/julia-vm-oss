# Tests for where-clause type variable binding from tuple arguments (Issue #2304)
# extract_type_bindings() must handle TupleOf patterns to bind type variables

using Test

# Homogeneous tuple: Tuple{T, T} where T
function sum_pair(t::Tuple{T, T}) where T
    return t[1] + t[2]
end

# Heterogeneous tuple: Tuple{T, S} where {T, S}
function first_of_pair(t::Tuple{T, S}) where {T, S}
    return t[1]
end

# Triple with uniform type: Tuple{T, T, T} where T
function sum_triple(t::Tuple{T, T, T}) where T
    return t[1] + t[2] + t[3]
end

@testset "Tuple where-clause type bindings (Issue #2304)" begin
    @testset "homogeneous pair binding" begin
        # T binds to Int64 from Tuple{Int64, Int64}
        @test sum_pair((1, 2)) == 3
        @test sum_pair((10, 20)) == 30

        # T binds to Float64 from Tuple{Float64, Float64}
        @test sum_pair((1.5, 2.5)) == 4.0
    end

    @testset "heterogeneous pair binding" begin
        # T binds to Int64, S binds to Int64
        @test first_of_pair((42, 99)) == 42

        # T binds to Float64, S binds to Float64
        @test first_of_pair((3.14, 2.72)) == 3.14
    end

    @testset "triple binding" begin
        # T binds to Int64 from Tuple{Int64, Int64, Int64}
        @test sum_triple((1, 2, 3)) == 6
        @test sum_triple((10, 20, 30)) == 60
    end
end

true
