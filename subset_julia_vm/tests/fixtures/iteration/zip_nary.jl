# Test zip with 3, 4, 5, 6, and 7 arguments (Issues #1990/#4281)

using Test

function test_zip3()
    a = [1, 2, 3]
    b = [10, 20, 30]
    c = [100, 200, 300]
    result = 0
    for (x, y, z) in zip(a, b, c)
        result = result + x + y + z
    end
    return result
end

function test_zip4()
    a = [1, 2]
    b = [10, 20]
    c = [100, 200]
    d = [1000, 2000]
    result = 0
    for (w, x, y, z) in zip(a, b, c, d)
        result = result + w + x + y + z
    end
    return result
end

function test_zip3_unequal()
    a = [1, 2, 3]
    b = [10, 20]
    c = [100, 200, 300, 400]
    count = 0
    for (x, y, z) in zip(a, b, c)
        count = count + 1
    end
    return count
end

function test_zip3_collect()
    a = [1, 2, 3]
    b = [4, 5, 6]
    c = [7, 8, 9]
    result = collect(zip(a, b, c))
    return length(result)
end

function test_zip5_collect()
    z = zip(1:2, 3:4, 5:6, 7:8, 9:10)
    return collect(z)
end

function test_zip6_collect()
    z = zip([1], [2.0], Int8[3], [true], ["x"], ['s'])
    return collect(z)
end

function test_zip7_collect()
    z = zip([1], [2.0], Int8[3], [true], ["x"], ['s'], UInt8[7])
    return collect(z)
end

@testset "zip with 3 arguments" begin
    # 1+10+100 + 2+20+200 + 3+30+300 = 111 + 222 + 333 = 666
    @test test_zip3() == 666
end

@testset "zip with 4 arguments" begin
    # 1+10+100+1000 + 2+20+200+2000 = 1111 + 2222 = 3333
    @test test_zip4() == 3333
end

@testset "zip3 with unequal lengths" begin
    @test test_zip3_unequal() == 2
end

@testset "zip3 collect" begin
    @test test_zip3_collect() == 3
end

@testset "zip5 collect preserves tuple element type" begin
    result = test_zip5_collect()
    @test typeof(result) == Vector{NTuple{5, Int64}}
    @test isa(result, Vector{NTuple{5, Int64}})
    @test result == [(1, 3, 5, 7, 9), (2, 4, 6, 8, 10)]
    @test length(zip(1:3, 1:2, 1:5, 1:4, 1:6)) == 2
end

@testset "zip6 collect preserves tuple element type" begin
    result = test_zip6_collect()
    @test typeof(result) == Vector{Tuple{Int64, Float64, Int8, Bool, String, Char}}
    @test eltype(result) == Tuple{Int64, Float64, Int8, Bool, String, Char}
    @test typeof(result[1]) == Tuple{Int64, Float64, Int8, Bool, String, Char}
    @test result == [(1, 2.0, Int8(3), true, "x", 's')]
    @test length(zip(1:3, 1:2, 1:5, 1:4, 1:6, 1:7)) == 2
end

@testset "zip7 collect preserves tuple element type" begin
    result = test_zip7_collect()
    @test typeof(result) == Vector{Tuple{Int64, Float64, Int8, Bool, String, Char, UInt8}}
    @test eltype(result) == Tuple{Int64, Float64, Int8, Bool, String, Char, UInt8}
    @test typeof(result[1]) == Tuple{Int64, Float64, Int8, Bool, String, Char, UInt8}
    @test result == [(1, 2.0, Int8(3), true, "x", 's', UInt8(7))]
    @test length(zip(1:3, 1:2, 1:5, 1:4, 1:6, 1:7, 1:8)) == 2
end

true
