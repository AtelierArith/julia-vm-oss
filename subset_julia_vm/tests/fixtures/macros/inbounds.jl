# Test @inbounds macro (related to Issue #890)
# - @inbounds is a no-op in SubsetJuliaVM (bounds checking is runtime)
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
