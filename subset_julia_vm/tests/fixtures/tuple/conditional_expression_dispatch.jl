# Tests for parametric tuple type tracking through conditional expressions (Issue #2319)
# julia_type_locals should track TupleOf types from if-else expression results

using Test

# Target functions with parametric tuple dispatch
function tuple_sum(t::Tuple{Int64, Int64})
    return t[1] + t[2]
end

function tuple_mixed(t::Tuple{Int64, String})
    return string(t[1]) * t[2]
end

# Tuple from if-else expression (both branches return same tuple type)
t1 = if true
    (1, 2)
else
    (3, 4)
end

t2 = if false
    (10, 20)
else
    (5, 6)
end

@testset "Tuple from if-else expression (Issue #2319)" begin
    # Both branches return Tuple{Int64, Int64}, so t1 and t2 should dispatch correctly
    @test tuple_sum(t1) == 3
    @test tuple_sum(t2) == 11
end

# Tuple from ternary expression
t3 = true ? (7, 8) : (9, 10)
t4 = false ? (100, 200) : (11, 12)

@testset "Tuple from ternary expression (Issue #2319)" begin
    @test tuple_sum(t3) == 15
    @test tuple_sum(t4) == 23
end

# Tuple from nested conditional
t5 = if true
    if true
        (13, 14)
    else
        (15, 16)
    end
else
    (17, 18)
end

@testset "Tuple from nested conditional (Issue #2319)" begin
    @test tuple_sum(t5) == 27
end

# Inline conditional in function argument
@testset "Inline conditional as function argument (Issue #2319)" begin
    # Direct inline conditional should dispatch correctly
    @test tuple_sum(if true; (1, 2) else (3, 4) end) == 3
    @test tuple_sum(true ? (5, 6) : (7, 8)) == 11
end

true
