# Prevention tests for parametric tuple type dispatch (Issue #1752)
# Verifies that functions with parametric tuple type annotations
# dispatch correctly, preventing regressions from Issue #1748.

using Test

# --- Function definitions (OUTSIDE @testset per scope rules) ---

# Basic parametric tuple dispatch
function tuple_sum(t::Tuple{Int64, Int64})
    return t[1] + t[2]
end

# Dispatch on different element types
function tuple_describe(t::Tuple{Int64, String})
    return t[1]
end

function tuple_describe(t::Tuple{String, Int64})
    return t[2]
end

# Tuple with Float64 elements
function tuple_product(t::Tuple{Float64, Float64})
    return t[1] * t[2]
end

# Parametric tuple with 3 elements
function triple_sum(t::Tuple{Int64, Int64, Int64})
    return t[1] + t[2] + t[3]
end

# Dispatch with Tuple{Any, Any} — single method (no ambiguity)
function process_any_pair(t::Tuple{Any, Any})
    return t[1]
end

# Tuple with Bool element
function check_flag(t::Tuple{Bool, Int64})
    if t[1]
        return t[2]
    end
    return 0
end

# Mixed numeric types in tuple
function mixed_numeric(t::Tuple{Int64, Float64})
    return t[1] + t[2]
end

# Single-element tuple dispatch
function single_elem(t::Tuple{Int64})
    return t[1] * 10
end

@testset "Parametric Tuple Dispatch (Issue #1752)" begin
    @testset "basic dispatch" begin
        @test tuple_sum((3, 4)) == 7
        @test tuple_sum((0, 0)) == 0
        @test tuple_sum((-1, 1)) == 0
    end

    @testset "dispatch on element type order" begin
        # Tuple{Int64, String} vs Tuple{String, Int64}
        @test tuple_describe((42, "hello")) == 42
        @test tuple_describe(("hello", 42)) == 42
    end

    @testset "float tuple dispatch" begin
        @test tuple_product((2.0, 3.0)) == 6.0
        @test tuple_product((0.5, 4.0)) == 2.0
    end

    @testset "triple element tuple" begin
        @test triple_sum((1, 2, 3)) == 6
        @test triple_sum((10, 20, 30)) == 60
    end

    @testset "any tuple dispatch" begin
        @test process_any_pair((1, 2)) == 1
        @test process_any_pair(("a", "b")) == "a"
    end

    @testset "bool element dispatch" begin
        @test check_flag((true, 42)) == 42
        @test check_flag((false, 42)) == 0
    end

    @testset "mixed numeric types" begin
        @test mixed_numeric((1, 2.5)) == 3.5
        @test mixed_numeric((0, 0.0)) == 0.0
    end

    @testset "single element tuple" begin
        @test single_elem((5,)) == 50
        @test single_elem((0,)) == 0
    end
end

true
