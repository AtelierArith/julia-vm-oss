# Regression test for the Memory<->Array equality boundary used by the
# binary_both dynamic dispatch fallback (Issue #3908). The fallback now
# reads memory cells through the public 1-indexed Memory boundary helper
# instead of touching MemoryValue::data directly, so the result must keep
# matching native Julia equality.

using Test

@testset "Memory and Array equality boundary (Issue #3908)" begin
    arr_int = [10, 20, 30]
    mem_int = Memory{Int64}(undef, 3)
    mem_int[1] = 10
    mem_int[2] = 20
    mem_int[3] = 30

    @test mem_int == arr_int
    @test arr_int == mem_int
    @test !(mem_int != arr_int)
    @test !(arr_int != mem_int)

    arr_float = [1.5, 2.5, 3.5]
    mem_float = Memory{Float64}(undef, 3)
    mem_float[1] = 1.5
    mem_float[2] = 2.5
    mem_float[3] = 3.5

    @test mem_float == arr_float
    @test arr_float == mem_float

    # Differ on the last cell only — must report inequality.
    mem_diff = Memory{Int64}(undef, 3)
    mem_diff[1] = 10
    mem_diff[2] = 20
    mem_diff[3] = 99
    @test mem_diff != arr_int
    @test arr_int != mem_diff
    @test !(mem_diff == arr_int)

    # Length mismatch — boundary must short-circuit to inequality without
    # reading past either side.
    mem_short = Memory{Int64}(undef, 2)
    mem_short[1] = 10
    mem_short[2] = 20
    @test mem_short != arr_int
    @test arr_int != mem_short
end

true
