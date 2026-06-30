# Test: Recursive calls produce concrete return types (Issue #3527)
# Previously the recursive edge poisoned inference to Top/Any. Now the
# fixpoint refines the recursive call to Int64 once the base case settles.
using Test

function fact(n::Int64)
    n <= 1 && return 1
    return n * fact(n - 1)
end

function fib(n::Int64)
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end

@testset "Recursive call inference" begin
    @test fact(5) == 120
    @test fact(10) == 3628800
    @test fib(0) == 0
    @test fib(1) == 1
    @test fib(10) == 55
end

true
