# Tests for TupleOf specificity-based dispatch (Issue #2302)
# Tuple{Int64, Int64} should be preferred over Tuple{Any, Any}

using Test

# Overlapping methods: specific vs general tuple
function process_pair(t::Tuple{Any, Any})
    return 0
end

function process_pair(t::Tuple{Int64, Int64})
    return t[1] + t[2]
end

# Mixed specificity: one element concrete, one Any
function mixed_pair(t::Tuple{Any, Any})
    return "any"
end

function mixed_pair(t::Tuple{Int64, Any})
    return "int_any"
end

@testset "TupleOf specificity dispatch (Issue #2302)" begin
    # Specific method should be selected over general
    @test process_pair((1, 2)) == 3
    @test process_pair((10, 20)) == 30

    # Mixed specificity: Int64,Any is more specific than Any,Any
    @test mixed_pair((42, "hello")) == "int_any"
end

true
