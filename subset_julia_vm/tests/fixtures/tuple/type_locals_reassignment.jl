# Tests for parametric tuple type preservation through reassignment (Issue #2305)
# julia_type_locals should track TupleOf types from variable reassignment

using Test

# Target functions with parametric tuple dispatch
function tuple_sum(t::Tuple{Int64, Int64})
    return t[1] + t[2]
end

function tuple_product(t::Tuple{Float64, Float64})
    return t[1] * t[2]
end

# Variable reassignment preserves Int tuple type
t1 = (3, 4)
t2 = t1

# Variable reassignment preserves Float tuple type
f1 = (2.0, 3.0)
f2 = f1

# Chain reassignment
t3 = t2

@testset "Tuple type preservation through reassignment (Issue #2305)" begin
    # Direct literal (baseline)
    @test tuple_sum((3, 4)) == 7
    @test tuple_product((2.0, 3.0)) == 6.0

    # Variable reassignment preserves type
    @test tuple_sum(t2) == 7
    @test tuple_product(f2) == 6.0

    # Chain reassignment (t3 = t2 = t1)
    @test tuple_sum(t3) == 7
end

true
