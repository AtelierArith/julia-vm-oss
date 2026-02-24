# Prevention test: Native Julia syntax that previously required workarounds (Issue #1794)
#
# Issue #1794 documented 6 unsupported features discovered during Julia Manual
# integration testing (PR #1793). All features have since been implemented.
# This test uses the ORIGINAL Julia syntax (no workarounds) to prevent regression.
#
# Features tested:
# 1. String interpolation: $var! (not $(var)!)
# 2. Default function arguments: function f(x, y=10)
# 3. begin/end compound expressions in assignment context
# 4. Mutable struct field compound assignment (+=, -=, etc.)
# 5. Bitwise operators (xor)
# 6. Float16 constructor

using Test

# --- Definitions OUTSIDE @testset per scope rules ---

# Feature 2: Default function arguments (previously required separate method definitions)
function greet_default(name, greeting="Hello")
    "$greeting, $name"
end

function add_with_default(a, b=10)
    a + b
end

function multi_default(a, b=2, c=3)
    a + b + c
end

# Feature 4: Mutable struct for compound field assignment
mutable struct PointMut
    x::Int64
    y::Int64
end

mutable struct Accumulator
    total::Float64
end

@testset "Prevention: Native Julia syntax (Issue #1794)" begin

    @testset "Feature 1: String interpolation \$var! boundary" begin
        # Previously: "Hello, $(name)!" was required (workaround)
        # Now: "Hello, $name!" works natively — $name stops before !
        name = "Julia"
        @test "Hello, $name!" == "Hello, Julia!"
        @test "$name!" == "Julia!"
        @test "Say $name!" == "Say Julia!"

        x = 42
        @test "value=$x." == "value=42."
        @test "($x)" == "(42)"
    end

    @testset "Feature 2: Default function arguments" begin
        # Previously: required defining separate methods manually
        # Now: function f(x, y=default) works natively
        @test greet_default("Julia") == "Hello, Julia"
        @test greet_default("Julia", "Hi") == "Hi, Julia"
        @test add_with_default(5) == 15
        @test add_with_default(5, 20) == 25
        @test multi_default(1) == 6
        @test multi_default(1, 10) == 14
        @test multi_default(1, 10, 100) == 111
    end

    @testset "Feature 3: begin/end compound expressions" begin
        # Previously: begin...end blocks in assignment context caused UndefVarError
        # Now: begin...end works as an expression returning the last value
        z = begin
            x = 1
            y = 2
            x + y
        end
        @test z == 3

        result = begin
            a = 10
            b = 20
            a * b
        end
        @test result == 200

        # Nested begin blocks
        w = begin
            inner = begin
                100
            end
            inner + 1
        end
        @test w == 101
    end

    @testset "Feature 4: Compound assignment on struct fields" begin
        # Previously: c.value += 1 caused UnsupportedAssignmentTarget
        # Workaround was: c.value = c.value + 1
        # Now: compound assignment on fields works natively
        p = PointMut(10, 20)
        p.x += 5
        @test p.x == 15
        p.y -= 10
        @test p.y == 10
        p.x *= 2
        @test p.x == 30

        acc = Accumulator(0.0)
        acc.total += 1.5
        acc.total += 2.5
        @test acc.total == 4.0
    end

    @testset "Feature 5: Bitwise operators" begin
        # Previously: bitwise operators were not supported
        # Now: xor and bit operations work
        @test xor(5, 3) == 6
        @test xor(255, 15) == 240
        @test xor(0, 0) == 0
        @test xor(true, false) == true
        @test xor(true, true) == false

        # Bit counting operations
        @test count_ones(7) == 3
        @test count_zeros(Int64(7)) == 61
    end

    @testset "Feature 6: Float16 constructor" begin
        # Previously: Float16() was an unknown function
        # Now: Float16 constructor works
        x = Float16(1.5)
        @test typeof(x) == Float16
        @test Float64(x) == 1.5

        y = Float16(0.0)
        @test y == Float16(0.0)
    end
end

true
