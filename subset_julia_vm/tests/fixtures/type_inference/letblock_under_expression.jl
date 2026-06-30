# Locals introduced inside a begin/let block under expression contexts
# (binary op, tuple, array, index) must be collected for inference.
# Issue #3537

using Test

function f3537_binop()
    y = (begin
        x = 41
        x
    end) + 1
    return y
end

function f3537_tuple()
    t = (begin
        a = 10
        a
    end, begin
        b = 20
        b
    end)
    return t
end

function f3537_array()
    arr = [begin
        z = 7
        z
    end]
    return arr[1]
end

function f3537_unary()
    y = -(begin
        w = 5
        w
    end)
    return y
end

@testset "LetBlock locals nested in expression positions" begin
    @test f3537_binop() == 42
    @test f3537_tuple() == (10, 20)
    @test f3537_array() == 7
    @test f3537_unary() == -5
end

true
