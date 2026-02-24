# Tests for julia_type_locals tracking function return values (Issue #2317)
# When a function returns a tuple, the parametric type should propagate to the caller

using Test

# Target function requiring parametric tuple dispatch
function tuple_sum(t::Tuple{Int64, Int64})
    return t[1] + t[2]
end

function tuple_product(t::Tuple{Float64, Float64})
    return t[1] * t[2]
end

# Functions that return parametric tuples
function make_int_pair()
    return (3, 4)
end

function make_float_pair()
    return (2.0, 3.0)
end

# Assign return values at global scope for julia_type_locals tracking
t1 = make_int_pair()
f1 = make_float_pair()

@testset "Function return tuple dispatch (Issue #2317)" begin
    # Direct literal (baseline)
    @test tuple_sum((3, 4)) == 7
    @test tuple_product((2.0, 3.0)) == 6.0

    # Function return value should carry parametric tuple type
    @test tuple_sum(t1) == 7
    @test tuple_product(f1) == 6.0
end

true
