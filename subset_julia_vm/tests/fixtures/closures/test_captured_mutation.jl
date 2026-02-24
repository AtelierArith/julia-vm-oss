# Test mutable captured variables
# Issue #1738: Ensure closures can mutate captured variables
#
# Note: Full mutation of captured variables (like Python's nonlocal or Rust's move semantics)
# may require special handling. These tests document expected behavior.

using Test

# Helper: counter closure (captures and mutates n)
function make_counter(start)
    n = start
    function increment()
        n = n + 1
        n
    end
    increment
end

# Helper: accumulator closure
function make_accumulator(initial)
    total = initial
    function add(x)
        total = total + x
        total
    end
    add
end

# Helper: toggle closure
function make_toggle(initial)
    state = initial
    function toggle()
        state = !state
        state
    end
    toggle
end

# Helper: closure pair sharing captured state
function make_getter_setter(initial)
    value = initial
    function getter()
        value
    end
    function setter(x)
        value = x
    end
    (getter, setter)
end

@testset "Mutable Captured Variables" begin
    @testset "counter closure" begin
        counter = make_counter(0)
        @test counter() == 1
        @test counter() == 2
        @test counter() == 3
    end

    @testset "independent counters" begin
        counter1 = make_counter(0)
        counter2 = make_counter(100)

        @test counter1() == 1
        @test counter2() == 101
        @test counter1() == 2
        @test counter2() == 102
    end

    @testset "accumulator closure" begin
        acc = make_accumulator(0)
        @test acc(5) == 5
        @test acc(3) == 8
        @test acc(2) == 10
    end

    @testset "toggle closure" begin
        toggle = make_toggle(false)
        @test toggle() == true
        @test toggle() == false
        @test toggle() == true
    end

    @testset "getter/setter pair" begin
        getter, setter = make_getter_setter(10)
        @test getter() == 10
        setter(20)
        @test getter() == 20
        setter(30)
        @test getter() == 30
    end
end

true
