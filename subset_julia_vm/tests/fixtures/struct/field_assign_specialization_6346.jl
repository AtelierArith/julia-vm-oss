# Lazy specialization of mutable-struct field updates (Issue #6346).
# A struct-mutating function called in a hot loop is specialized to a typed
# SetField fast path instead of falling back to the interpreter. This fixture
# pins the observable behaviour so the specialized and fallback paths agree.

using Test

mutable struct Particle6346
    x::Float64
    vx::Float64
end

function step_particle_6346!(p, dt)
    p.x = p.x + p.vx * dt
    return p.x
end

function simulate_6346(n)
    p = Particle6346(0.0, 1.5)
    s = 0.0
    for i in 1:n
        s = step_particle_6346!(p, 0.1)
    end
    return s
end

mutable struct Counter6346
    count::Int64
end

function bump_6346!(c)
    c.count = c.count + 1
    return c.count
end

function run_counter_6346(n)
    c = Counter6346(0)
    total = 0
    for i in 1:n
        total = bump_6346!(c)
    end
    return total
end

# An Int literal assigned to a Float64 field must coerce to 2.0, exactly like the
# interpreter's typed field-assignment path.
mutable struct Box6346
    v::Float64
end

function set_box_6346!(b)
    b.v = 2
    return b.v
end

# A field update whose value contains an n-ary product `k * b.x * dt` (which the
# parser spells as `*(k, b.x, dt)`); the whole function must still specialize.
mutable struct Body6346
    x::Float64
    v::Float64
end

function step_body_6346!(b, dt, k)
    b.v = b.v - k * b.x * dt
    b.x = b.x + b.v * dt
    return b.x
end

function integrate_6346(steps)
    b = Body6346(1.0, 0.0)
    acc = 0.0
    for _ in 1:steps
        acc += step_body_6346!(b, 0.001, 4.0)
    end
    return acc
end

@testset "field-assign specialization (Issue #6346)" begin
    @test simulate_6346(1000) == 150.0000000000028
    @test run_counter_6346(5000) == 5000

    c = Counter6346(41)
    @test bump_6346!(c) == 42
    @test c.count == 42

    b = Box6346(0.0)
    @test set_box_6346!(b) === 2.0
    @test b.v === 2.0

    @test integrate_6346(2000) == -380.0545410490795
end

true
