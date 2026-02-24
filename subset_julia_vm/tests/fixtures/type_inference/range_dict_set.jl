# Test: Range, Dict, Set type inference
using Test

# Functions must be defined OUTSIDE @testset block per project guidelines
function sum_range()
    total = 0
    for i in 1:10  # i should be Int64
        total += i
    end
    total
end

function process_dict()
    d = Dict("a" => 1, "b" => 2)
    result = 0
    for (k, v) in d  # k: String, v: Int64
        result += v
    end
    result
end

function process_set()
    s = Set([1, 2, 3])
    total = 0
    for x in s  # x: Int64
        total += x
    end
    total
end

@testset "Range type inference" begin
    @test sum_range() == 55
end

@testset "Dict type inference" begin
    @test process_dict() == 3
end

@testset "Set type inference" begin
    @test process_set() == 6
end

true
