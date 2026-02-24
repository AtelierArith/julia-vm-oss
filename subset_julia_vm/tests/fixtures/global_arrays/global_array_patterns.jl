using Test

# Global const arrays must be at top-level scope.

const PRIMES = [2, 3, 5, 7, 11, 13]
const ACC = [0]
const LOG2 = Int64[]
const TEMPS = [36.5, 37.0, 36.8, 37.2]

function add_to_acc(x)
    ACC[1] += x
end

function log_value_idx(i, v)
    if i > length(LOG2)
        push!(LOG2, v)
    else
        LOG2[i] = v
    end
end

@testset "Global array patterns" begin
    @testset "read-only global const arrays" begin
        @test length(PRIMES) == 6
        @test PRIMES[1] == 2
        @test PRIMES[end] == 13
        @test sum(PRIMES) == 41
    end

    @testset "global const array mutation via functions" begin
        add_to_acc(10)
        add_to_acc(20)
        add_to_acc(5)
        @test ACC[1] == 35
    end

    @testset "global const array element mutation" begin
        const_arr = [10, 20, 30]
        const_arr[2] = 99
        @test const_arr[2] == 99
        @test const_arr[1] == 10
        @test const_arr[3] == 30
    end

    @testset "Float64 global array" begin
        @test minimum(TEMPS) == 36.5
        @test maximum(TEMPS) == 37.2
        @test length(TEMPS) == 4
    end

    @testset "push! via function on typed empty global array (Issue #3121)" begin
        log_value_idx(1, 100)
        log_value_idx(2, 200)
        @test length(LOG2) == 2
        @test LOG2[1] == 100
        @test LOG2[2] == 200
    end
end

true
