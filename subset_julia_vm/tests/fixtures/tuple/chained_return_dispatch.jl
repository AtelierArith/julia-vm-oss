# Tests for chained function call tuple dispatch (Issue #2323)
# Tests that parametric tuple type propagates through multiple function calls

using Test

# Target function requiring parametric tuple dispatch
function tuple_sum(t::Tuple{Int64, Int64})
    return t[1] + t[2]
end

# Inner function that creates a tuple
function make_pair()
    return (1, 2)
end

# Outer function that wraps the inner function
function wrap_pair()
    return make_pair()
end

# Double-wrapped for deeper chain
function double_wrap_pair()
    return wrap_pair()
end

# Assign chained return values at global scope
t1 = wrap_pair()
t2 = double_wrap_pair()

@testset "Chained function return tuple dispatch (Issue #2323)" begin
    # Single wrap
    @test tuple_sum(t1) == 3

    # Double wrap
    @test tuple_sum(t2) == 3

    # Direct call chain (inline)
    @test tuple_sum(wrap_pair()) == 3
    @test tuple_sum(double_wrap_pair()) == 3
end

# Test mixed-type tuples through chains
function make_mixed()
    return (1, 2.0)
end

function wrap_mixed()
    return make_mixed()
end

function mixed_sum(t::Tuple{Int64, Float64})
    return Float64(t[1]) + t[2]
end

t3 = wrap_mixed()

@testset "Mixed-type chained tuple dispatch (Issue #2323)" begin
    @test mixed_sum(t3) == 3.0
    @test mixed_sum(wrap_mixed()) == 3.0
end

true
