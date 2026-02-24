# Test foldr (right-associative fold)

using Test

function mysub(a, b)
    a - b
end

function myadd(a, b)
    a + b
end

@testset "foldr - right-associative fold (Issue #351)" begin

    # Define a custom subtract function

    # Test 1: foldr with subtract
    # foldr(-, [1,2,3,4]) = 1-(2-(3-4)) = 1-(2-(-1)) = 1-3 = -2
    arr1 = [1, 2, 3, 4]
    r1 = foldr(mysub, arr1)
    check1 = r1 == -2

    # Test 2: foldr with initial value
    # foldr(-, [1,2,3], 0) = 1-(2-(3-0)) = 1-(2-3) = 1-(-1) = 2
    arr2 = [1, 2, 3]
    r2 = foldr(mysub, arr2, 0)
    check2 = r2 == 2

    # Define add function

    # Test 3: foldr with add (same result as foldl for associative ops)
    arr3 = [1, 2, 3, 4, 5]
    r3 = foldr(myadd, arr3)
    check3 = r3 == 15

    # All checks must pass
    @test (check1 && check2 && check3)
end

true  # Test passed
