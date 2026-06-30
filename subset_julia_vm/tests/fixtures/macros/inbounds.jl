# Test @inbounds macro (related to Issue #890)
# - @inbounds marks local indexing expressions for inbounds codegen (Issue #4286)
# - @inbounds expr should execute normally
# - @inbounds for loop should execute normally

using Test

@testset "@inbounds with for loop" begin
    arr = [1, 2, 3, 4, 5]
    sum = 0
    @inbounds for i in 1:length(arr)
        sum += arr[i]
    end
    @test sum == 15
end

@testset "@inbounds with array mutation" begin
    arr = zeros(Int, 5)
    @inbounds for i in 1:5
        arr[i] = i * 10
    end
    @test arr == [10, 20, 30, 40, 50]
end

@testset "@inbounds statement body indexing" begin
    vals = Float64[1.0, 2.0, 3.0]
    idxs = [3, 1]
    @inbounds for i in idxs
        vals[i] = vals[i] + 10.0
    end
    @test vals == Float64[11.0, 2.0, 13.0]
end

@testset "@inbounds with direct indexing expressions" begin
    arr = Int32[10, 20, 30]
    @test @inbounds arr[2] == Int32(20)
    @test @inbounds getindex(arr, 3) == Int32(30)
    @test @inbounds Base.getindex(arr, 1) == Int32(10)

    vals = Float64[1.0, 2.0, 3.0]
    @inbounds vals[2] = 4.5
    @test vals == Float64[1.0, 4.5, 3.0]
    @inbounds setindex!(vals, 6.5, 3)
    @test vals == Float64[1.0, 4.5, 6.5]
end

@testset "@inbounds with while loop" begin
    sum = 0
    i = 1
    @inbounds while i <= 5
        sum += i
        i += 1
    end
    @test sum == 15
end

@testset "@inbounds with if statement" begin
    x = 10
    result = 0
    @inbounds if x > 5
        result = x * 2
    end
    @test result == 20
end

true
