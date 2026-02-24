# Test zip with 3 and 4 arguments (Issue #1990)

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

true
