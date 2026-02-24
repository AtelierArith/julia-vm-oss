# Test that builtin type names are typed correctly even when they have methods
# This ensures proper dispatch for functions like nameof(t::Type)
# Related to Issue #1692 and #1701: Type inference order for builtin type names vs method_tables

using Test

# Define test functions for type vs function dispatch
function test_type_dispatch(t::Type)
    return :type
end

function test_type_dispatch(f::Function)
    return :function
end

@testset "Builtin type inference priority" begin
    # Test nameof for builtin types
    # These types have methods defined but should still dispatch to Type{T}
    @test nameof(Tuple) == :Tuple
    @test nameof(Array) == :Array
    @test nameof(Dict) == :Dict
    @test nameof(Int64) == :Int64
    @test nameof(Float64) == :Float64
    @test nameof(String) == :String

    # Test that type dispatch works correctly for builtin types
    # Tuple has methods (e.g., Tuple(ci::CartesianIndex)) but should be typed as TypeOf(Tuple)
    @test test_type_dispatch(Tuple) == :type
    @test test_type_dispatch(Array) == :type
    @test test_type_dispatch(Int64) == :type
    @test test_type_dispatch(Float64) == :type

    # Test that function dispatch works correctly for actual functions
    @test test_type_dispatch(sin) == :function
    @test test_type_dispatch(cos) == :function
end

true
