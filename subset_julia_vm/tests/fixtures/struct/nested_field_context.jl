# Nested field assignment in loop contexts (Issue #2314)
# Tests the four parallel code paths for nested field assignment

using Test

mutable struct Inner
    value::Int64
end

mutable struct Outer
    inner::Inner
end

mutable struct Deep
    outer::Outer
end

# Define closures at module scope (global variables needed for closure capture testing)
# These closures modify nested fields on global variables
module_outer = Outer(Inner(0))
module_deep = Deep(Outer(Inner(0)))

closure_set = () -> begin
    module_outer.inner.value = 42
    module_outer.inner.value
end

closure_add = () -> begin
    module_outer.inner.value += 5
    module_outer.inner.value
end

closure_deep_set = () -> begin
    module_deep.outer.inner.value = 200
    module_deep.outer.inner.value
end

closure_deep_add = () -> begin
    module_deep.outer.inner.value += 25
    module_deep.outer.inner.value
end

@testset "Nested field assignment in closure context (Issue #2314)" begin
    # Reset and test basic closure
    module_outer.inner.value = 10
    @test closure_set() == 42
    @test module_outer.inner.value == 42

    # Reset and test compound closure
    module_outer.inner.value = 10
    @test closure_add() == 15
    @test module_outer.inner.value == 15

    # Reset and test deep nesting closure
    module_deep.outer.inner.value = 100
    @test closure_deep_set() == 200
    @test module_deep.outer.inner.value == 200

    # Reset and test deep compound closure
    module_deep.outer.inner.value = 50
    @test closure_deep_add() == 75
    @test module_deep.outer.inner.value == 75
end

@testset "Nested field assignment in for loop context (Issue #2314)" begin
    # Basic loop with nested assignment
    o = Outer(Inner(0))
    for i in 1:5
        o.inner.value = i
    end
    @test o.inner.value == 5

    # Loop with compound nested assignment
    o2 = Outer(Inner(0))
    for i in 1:4
        o2.inner.value += i
    end
    @test o2.inner.value == 10  # 1+2+3+4

    # Loop with deep nesting
    d = Deep(Outer(Inner(1)))
    for i in 1:3
        d.outer.inner.value *= 2
    end
    @test d.outer.inner.value == 8  # 1*2*2*2

    # Loop with multiple nested field operations
    o3 = Outer(Inner(100))
    for i in 1:3
        o3.inner.value -= 10
        o3.inner.value += 5
    end
    @test o3.inner.value == 85  # 100 + 3*(-10+5) = 100 - 15

    # Nested loop with nested field
    o4 = Outer(Inner(0))
    for i in 1:2
        for j in 1:3
            o4.inner.value += 1
        end
    end
    @test o4.inner.value == 6
end

@testset "Nested field assignment in while loop context (Issue #2314)" begin
    # While loop with nested assignment
    o = Outer(Inner(0))
    count = 0
    while count < 5
        o.inner.value += 1
        count += 1
    end
    @test o.inner.value == 5

    # While loop with deep nesting
    d = Deep(Outer(Inner(10)))
    iterations = 0
    while d.outer.inner.value > 0
        d.outer.inner.value -= 2
        iterations += 1
    end
    @test iterations == 5
end

true
