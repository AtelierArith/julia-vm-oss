# Test symbol default values in keyword arguments (Issue #830)

using Test

# Function definitions must be outside @testset

# Test 1 & 2: Symbol default value should preserve its type
function test_symbol_kwarg(x; color=:red)
    typeof(color)
end

# Test 3: Multiple symbol kwargs
function test_multi_symbol_kwargs(; a=:foo, b=:bar)
    (typeof(a), typeof(b))
end

# Test 4: Symbol comparison with default
function get_color_code(; color=:red)
    if color === :red
        return 1
    elseif color === :green
        return 2
    elseif color === :blue
        return 3
    else
        return 0
    end
end

@testset "Symbol default values in keyword arguments" begin
    # Test 1: Symbol default value should preserve its type
    @test test_symbol_kwarg("hello") === Symbol

    # Test 2: Passing a symbol explicitly should work
    @test test_symbol_kwarg("hello"; color=:blue) === Symbol

    # Test 3: Multiple symbol kwargs
    @test test_multi_symbol_kwargs() == (Symbol, Symbol)
    @test test_multi_symbol_kwargs(; a=:baz) == (Symbol, Symbol)

    # Test 4: Symbol comparison with default
    @test get_color_code() == 1
    @test get_color_code(; color=:red) == 1
    @test get_color_code(; color=:green) == 2
    @test get_color_code(; color=:blue) == 3
    @test get_color_code(; color=:unknown) == 0
end

true
