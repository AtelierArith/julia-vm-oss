# Tests for 3-element tuple specificity dispatch (Issue #2321)
# Verifies that element-wise sum scoring works correctly for longer tuples

using Test

# 4 methods with varying specificity levels
function f3(t::Tuple{Any, Any, Any})
    return "any"
end

function f3(t::Tuple{Int64, Any, Any})
    return "int_any_any"
end

function f3(t::Tuple{Int64, Int64, Any})
    return "int_int_any"
end

function f3(t::Tuple{Int64, Int64, Int64})
    return "int_int_int"
end

@testset "3-element tuple specificity dispatch (Issue #2321)" begin
    # Most specific: all Int64
    @test f3((1, 2, 3)) == "int_int_int"

    # Two Int64, one String (matches Int64, Int64, Any)
    @test f3((1, 2, "x")) == "int_int_any"

    # One Int64, two non-Int64 (matches Int64, Any, Any)
    @test f3((1, "x", "y")) == "int_any_any"

    # No Int64 (matches Any, Any, Any)
    @test f3(("x", "y", "z")) == "any"
end

# Test tuple vs non-tuple dispatch (ensure TupleOf score doesn't interfere)
function g(x::Any)
    return "any"
end

function g(x::Tuple{Int64})
    return "tuple_int"
end

@testset "Tuple vs non-tuple dispatch" begin
    # Parametric tuple should match over Any
    @test g((1,)) == "tuple_int"

    # Non-tuple should match Any
    @test g(42) == "any"
    @test g("hello") == "any"
end

# Test middle-position specificity (Any in different positions)
function mid(t::Tuple{Any, Any, Any})
    return "any_any_any"
end

function mid(t::Tuple{Any, Int64, Any})
    return "any_int_any"
end

function mid(t::Tuple{Int64, Int64, Any})
    return "int_int_any"
end

@testset "Middle position specificity" begin
    # Int64 in middle only
    @test mid(("a", 1, "b")) == "any_int_any"

    # Int64 in first two positions
    @test mid((1, 2, "c")) == "int_int_any"

    # All non-Int64
    @test mid(("a", "b", "c")) == "any_any_any"
end

true
