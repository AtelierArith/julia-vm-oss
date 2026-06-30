using Test

map_memory_plus_ten_4052(x) = x + 10

function map_memory_widen_4052(x)
    if x == 2
        return 2.5
    end
    return x
end

map_memory_plus_one_4052(x) = x + 1

@testset "Memory map uses collect_similar (Issues #4052/#4018)" begin
    m = Memory{Int64}(undef, 3)
    m[1] = 1
    m[2] = 2
    m[3] = 3

    values = map(map_memory_plus_ten_4052, m)
    @test typeof(values) == Memory{Int64}
    @test eltype(values) == Int64
    @test length(values) == 3
    @test values[1] == 11
    @test values[2] == 12
    @test values[3] == 13
    values[2] = 20
    @test values[2] == 20

    widened = map(map_memory_widen_4052, m)
    @test typeof(widened) == Memory{Real}
    @test eltype(widened) == Real
    @test length(widened) == 3
    @test widened[1] == 1
    @test widened[2] == 2.5
    @test widened[3] == 3

    empty = Memory{Int64}(undef, 0)
    empty_values = map(map_memory_plus_one_4052, empty)
    @test typeof(empty_values) == Memory{Int64}
    @test eltype(empty_values) == Int64
    @test length(empty_values) == 0
end

true
