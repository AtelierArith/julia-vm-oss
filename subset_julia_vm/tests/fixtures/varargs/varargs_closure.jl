# Prevention test: Closure capturing varargs-derived variables (Issue #1722)
# Verifies that closures can capture variables computed from varargs parameters

using Test

# Pattern 1: Closure captures a variable derived from varargs via sum()
function make_adder(base...)
    base_sum = sum(base)
    function adder(x)
        x + base_sum
    end
    adder
end

# Pattern 2: Closure captures varargs length
function make_counter(items...)
    n = length(items)
    function get_count()
        n
    end
    get_count
end

# Pattern 3: Closure captures varargs tuple directly
function make_first_getter(args...)
    t = args
    function get_first()
        t[1]
    end
    get_first
end

# Pattern 4: Closure captures multiple varargs-derived values
function make_offset_adder(offsets...)
    s = sum(offsets)
    n = length(offsets)
    function apply(x)
        x + s + n
    end
    apply
end

# Pre-compute results outside @testset
add_base = make_adder(10, 20)
r_adder = add_base(5)

counter = make_counter(1, 2, 3, 4, 5)
r_counter = counter()

first_getter = make_first_getter(42, 99)
r_first = first_getter()

offset_fn = make_offset_adder(10, 20, 30)
r_offset = offset_fn(100)

@testset "Closure capturing varargs-derived variables (Issue #1722)" begin
    # Closure over sum of varargs
    @test r_adder == 35

    # Closure over length of varargs
    @test r_counter == 5

    # Closure over varargs tuple itself
    @test r_first == 42

    # Closure capturing multiple varargs-derived values (sum=60, len=3)
    @test r_offset == 163
end

true
