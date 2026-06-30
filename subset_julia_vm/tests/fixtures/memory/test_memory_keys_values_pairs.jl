using Test

@testset "Memory keys, values, and pairs direct storage" begin
    m = Memory{Int64}(undef, 3)

    for i in 1:3
        m[i] = i * 10
    end

    key_sum = 0
    for i in keys(m)
        key_sum = key_sum + i
    end
    @test length(keys(m)) == 3
    @test key_sum == 6

    @test values(m) === m
    @test collect(values(m)) == [10, 20, 30]

    p = pairs(m)
    @test occursin("Pairs", string(typeof(p)))
    @test length(p) == 3
    @test p[1] == 10
    @test p[2] == 20
    @test p[3] == 30
    @test values(p) === m
    @test length(keys(p)) == 3
    @test length(axes(p)[1]) == 3

    first_pair = iterate(p)[1]
    @test first_pair.first == 1
    @test first_pair.second == 10

    pair_sum = 0
    value_sum = 0
    for (i, v) in pairs(m)
        pair_sum = pair_sum + i
        value_sum = value_sum + v
    end
    @test pair_sum == 6
    @test value_sum == 60

    m[2] = 200
    @test values(m)[2] == 200
    updated_sum = 0
    for (i, v) in pairs(m)
        updated_sum = updated_sum + i * v
    end
    @test updated_sum == 1 * 10 + 2 * 200 + 3 * 30
end

true
