using Test

@testset "Memory dynamic arithmetic" begin
    m = Memory{Int64}(undef, 3)
    n = Memory{Int64}(undef, 3)
    f = Memory{Float64}(undef, 3)

    for i in 1:3
        m[i] = i
        n[i] = 10 * i
        f[i] = i + 0.5
    end

    add_mem = m + n
    @test length(add_mem) == 3
    @test add_mem[1] == 11
    @test add_mem[2] == 22
    @test add_mem[3] == 33

    sub_mem = m - n
    @test sub_mem[1] == -9
    @test sub_mem[2] == -18
    @test sub_mem[3] == -27

    mixed_add = m + f
    @test mixed_add[1] == 2.5
    @test mixed_add[2] == 4.5
    @test mixed_add[3] == 6.5

    a = [10, 20, 30]
    mem_array_add = m + a
    array_mem_add = a + m
    @test mem_array_add[1] == 11
    @test mem_array_add[2] == 22
    @test mem_array_add[3] == 33
    @test array_mem_add[1] == 11
    @test array_mem_add[2] == 22
    @test array_mem_add[3] == 33

    mem_div = m / 2
    @test mem_div[1] == 0.5
    @test mem_div[2] == 1.0
    @test mem_div[3] == 1.5
end

true
