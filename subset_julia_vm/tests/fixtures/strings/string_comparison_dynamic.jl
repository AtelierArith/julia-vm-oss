# Test string comparison with dynamic dispatch (Issue #1218)
# Verifies that string == string returns Bool when one operand has type Any at compile time

using Test

# Helper struct and functions to create dynamic string values
struct StringHolder
    msg::String
end

function get_msg(s::StringHolder)
    return s.msg
end

function inner_string()
    return "hello"
end

function outer_string()
    return inner_string()
end

@testset "String comparison with dynamic dispatch" begin
    # Test 1: Direct string comparison (both types known at compile time)
    @testset "Static string comparison" begin
        a = "hello"
        b = "hello"
        c = "world"
        @test a == b
        @test !(a == c)
        @test a != c
    end

    # Test 2: Struct field string comparison (type becomes Any through function call)
    @testset "Struct field string comparison" begin
        holder = StringHolder("hello")
        result = get_msg(holder)
        @test result == "hello"
        @test !(result == "world")
        @test result != "world"
    end

    # Test 3: Nested function call string comparison
    @testset "Nested function string comparison" begin
        result = outer_string()
        @test result == "hello"
        @test !(result == "world")
    end

    # Test 4: Both operands are dynamic
    @testset "Both operands dynamic" begin
        h1 = StringHolder("test")
        h2 = StringHolder("test")
        h3 = StringHolder("other")
        r1 = get_msg(h1)
        r2 = get_msg(h2)
        r3 = get_msg(h3)
        @test r1 == r2
        @test !(r1 == r3)
        @test r1 != r3
    end

    # Test 5: String inequality (!=) with dynamic dispatch
    @testset "String inequality dynamic" begin
        holder = StringHolder("hello")
        result = get_msg(holder)
        @test result != "world"
        @test !(result != "hello")
    end
end

true
