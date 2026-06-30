using Test

dyn_add(x::Any, y::Any) = x + y
dyn_sub(x::Any, y::Any) = x - y
dyn_mul(x::Any, y::Any) = x * y
dyn_div(x::Any, y::Any) = x / y
dyn_eq(x::Any, y::Any) = x == y
dyn_ne(x::Any, y::Any) = x != y

@testset "Memory dynamic Any binary fallback" begin
    m = Memory{Int64}(undef, 3)
    n = Memory{Int64}(undef, 3)

    for i in 1:3
        m[i] = i
        n[i] = 10 * i
    end

    a = [1, 2, 3]

    add_mem = dyn_add(m, n)
    @test add_mem[1] == 11
    @test add_mem[2] == 22
    @test add_mem[3] == 33

    sub_mem = dyn_sub(m, n)
    @test sub_mem[1] == -9
    @test sub_mem[2] == -18
    @test sub_mem[3] == -27

    @test dyn_eq(m, a)
    @test dyn_eq(a, m)
    @test !dyn_ne(m, a)

    left_scaled = dyn_mul(2, m)
    right_scaled = dyn_mul(m, 2)
    @test left_scaled[1] == 2
    @test left_scaled[2] == 4
    @test left_scaled[3] == 6
    @test right_scaled[1] == 2
    @test right_scaled[2] == 4
    @test right_scaled[3] == 6

    divided = dyn_div(m, 2)
    @test divided[1] == 0.5
    @test divided[2] == 1.0
    @test divided[3] == 1.5

    @test_throws MethodError dyn_mul(m, n)
end

true
