using Test

# Frame-pool reuse regression (Issue #5172).
#
# The VM recycles retired call frames instead of allocating fresh ones on every
# call. A recycled frame must be fully reset: leftover local bindings, captured
# variables, and slot contents from a previous call must NEVER be visible in the
# next call that reuses the same frame. These tests deliberately interleave
# calls whose locals differ so that any stale state would produce a wrong value.

# Deep self-recursion: every frame is pushed then popped (and recycled).
fib(n) = n < 2 ? n : fib(n - 1) + fib(n - 2)

# Mutual recursion exercises recycling across two different functions whose
# frames share the pool.
is_even(n) = n == 0 ? true : is_odd(n - 1)
is_odd(n) = n == 0 ? false : is_even(n - 1)

# A function that only binds an extra local on one branch. If a recycled frame
# kept the local from a previous (then-branch) call, the else-branch call could
# observe a stale value instead of recomputing.
function branchy(n)
    if n % 2 == 0
        tmp = n * 100
        return tmp + 1
    else
        return n + 7
    end
end

# Tight call loop: reuse the pool many times in succession.
function sum_squares(n)
    total = 0
    for i in 1:n
        total += square(i)
    end
    return total
end
square(x) = x * x

@testset "frame pool reuse - deep recursion" begin
    @test fib(10) == 55
    @test fib(15) == 610
    @test fib(20) == 6765
end

@testset "frame pool reuse - mutual recursion" begin
    @test is_even(100) == true
    @test is_odd(100) == false
    @test is_even(0) == true
    @test is_odd(1) == true
end

@testset "frame pool reuse - branch-local must not leak" begin
    # Interleave even (binds tmp) and odd (does not) calls. A leaked tmp would
    # corrupt the odd-branch result.
    results = Int[]
    for n in 1:8
        push!(results, branchy(n))
    end
    # n=1:8 -> odd:1+7=8, even:2*100+1=201, odd:3+7=10, even:401, odd:12, even:601, odd:14, even:801
    @test results == [8, 201, 10, 401, 12, 601, 14, 801]
end

@testset "frame pool reuse - tight call loop" begin
    @test sum_squares(5) == 55      # 1+4+9+16+25
    @test sum_squares(10) == 385
end

true
