# Test @simd macro (Issue #890)
# - @simd is a no-op in SubsetJuliaVM (no JIT/LLVM vectorization)
# - @simd for loop should execute normally
# - @simd ivdep for loop should also execute normally

using Test

@testset "@simd basic for loop" begin
    # Simple sum with @simd
    sum = 0
    @simd for i in 1:10
        sum += i
    end
    @test sum == 55
end

@testset "@simd with array accumulation" begin
    # Sum array elements with @simd
    arr = [1, 2, 3, 4, 5]
    total = 0
    @simd for i in 1:length(arr)
        total += arr[i]
    end
    @test total == 15
end

@testset "@simd with computation" begin
    # Compute squares with @simd
    squares = zeros(Int, 5)
    @simd for i in 1:5
        squares[i] = i * i
    end
    @test squares == [1, 4, 9, 16, 25]
end

@testset "@simd ivdep variant" begin
    # @simd ivdep should also work (no-op in SubsetJuliaVM)
    sum = 0
    @simd ivdep for i in 1:5
        sum += i
    end
    @test sum == 15
end

@testset "@simd with nested computation" begin
    # More complex computation
    result = 0.0
    @simd for i in 1:4
        result += i * 2.5
    end
    # 1*2.5 + 2*2.5 + 3*2.5 + 4*2.5 = 2.5 + 5.0 + 7.5 + 10.0 = 25.0
    @test result == 25.0
end

true
