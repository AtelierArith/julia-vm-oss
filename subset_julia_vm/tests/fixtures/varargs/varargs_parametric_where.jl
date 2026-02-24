# Test varargs parameters with `where T` type parameters (Issue #1684)
# Parametric typed varargs like (x::T, ys::T...) where T should work correctly

using Test

# Parametric type varargs with where clause - all args same type
function sum_same_type(x::T, ys::T...) where T
    total = x
    for y in ys
        total += y
    end
    total
end

function count_same_type(x::T, ys::T...) where T
    1 + length(ys)
end

# Multiple type parameters with varargs
function pair_with_values(first::S, second::T, rest::T...) where {S,T}
    # Return length of rest plus tuple of types
    (length(rest), typeof(first), typeof(second))
end

# Varargs only with type parameter
function concat_all(vs::T...) where T
    result = T[]
    for v in vs
        push!(result, v)
    end
    result
end

@testset "Parametric type varargs with where clause" begin
    # Test sum_same_type with Int64
    @test sum_same_type(1, 2, 3) == 6
    @test sum_same_type(10, 20, 30, 40) == 100
    @test sum_same_type(5) == 5

    # Test sum_same_type with Float64
    @test sum_same_type(1.0, 2.0, 3.0) == 6.0
    @test sum_same_type(1.5, 2.5) == 4.0

    # Test count_same_type
    @test count_same_type(1) == 1
    @test count_same_type(1, 2) == 2
    @test count_same_type(1, 2, 3, 4, 5) == 5

    # Test pair_with_values with mixed types
    r1 = pair_with_values("hello", 1, 2, 3)
    @test r1[1] == 2  # length of rest (2, 3)

    # Test concat_all
    @test concat_all(1, 2, 3) == [1, 2, 3]
    @test concat_all(1.0, 2.0) == [1.0, 2.0]
end

true
