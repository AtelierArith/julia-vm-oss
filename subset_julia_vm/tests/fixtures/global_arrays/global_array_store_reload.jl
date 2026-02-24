using Test

# Issue #3131: StoreArray-for-globals slotization hazard
# Tests that global arrays remain visible and modifiable after store/reload patterns

const STORE_A = [1, 2, 3]

function double_first_element()
    STORE_A[1] = STORE_A[1] * 2
end

function read_after_write(idx)
    STORE_A[idx] = STORE_A[idx] + 100
    return STORE_A[idx]
end

@testset "StoreArray global mutation (Issue #3131)" begin
    @testset "In-place element mutation" begin
        double_first_element()
        @test STORE_A[1] == 2
        double_first_element()
        @test STORE_A[1] == 4
    end

    @testset "Read-after-write in same function" begin
        result = read_after_write(2)
        @test result == 102
        @test STORE_A[2] == 102
    end
end

const MULTI_G = [10, 20, 30]

function swap_elements(i, j)
    tmp = MULTI_G[i]
    MULTI_G[i] = MULTI_G[j]
    MULTI_G[j] = tmp
end

@testset "Multi-step global array mutation" begin
    swap_elements(1, 3)
    @test MULTI_G[1] == 30
    @test MULTI_G[3] == 10
    @test MULTI_G[2] == 20
end

const ACC_INT = Int64[]

function accumulate_and_sum()
    push!(ACC_INT, 10)
    push!(ACC_INT, 20)
    push!(ACC_INT, 30)
    return ACC_INT[1] + ACC_INT[2] + ACC_INT[3]
end

@testset "Global array push and read in same function" begin
    s = accumulate_and_sum()
    @test s == 60
    @test length(ACC_INT) == 3
end

true
