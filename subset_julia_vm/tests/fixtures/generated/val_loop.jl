# Test: Using Val{N} type parameter as integer value
# Phase 2: N from where clause should be usable as integer in fallback
#
# This test verifies that N from Val{N} where N can be used as an integer
# in the function body. In SubsetJuliaVM Phase 1, the @generated fallback
# pattern executes the else branch.

using Test

function make_sum(::Val{N}) where N
    result = 0
    if @generated
        # Skipped in SubsetJuliaVM Phase 1
        result = -1
    else
        # Fallback: use N as loop bound
        for i in 1:N
            result = result + i
        end
    end
    result
end

@testset "Val{N} where N is used as integer in loop" begin


    # Call with Val{5}() - N should be bound to 5
    r = make_sum(Val{5}())
    println("make_sum(Val{5}()) = ", r)
    @assert r == 15  # 1+2+3+4+5 = 15

    # Call with Val{3}()
    r2 = make_sum(Val{3}())
    println("make_sum(Val{3}()) = ", r2)
    @assert r2 == 6  # 1+2+3 = 6

    # Return true if all tests passed
    @test (true)
end

true  # Test passed
