# Test foldl (left-associative fold)
# foldl is an alias for reduce

using Test

function mysub(a, b)
    a - b
end

function myadd(a, b)
    a + b
end

@testset "foldl - left-associative fold (Issue #351)" begin

    # Define a custom subtract function

    # Test 1: foldl with subtract
    # foldl(-, [1,2,3,4]) = ((1-2)-3)-4 = -8
    arr1 = [1, 2, 3, 4]
    r1 = foldl(mysub, arr1)
    check1 = r1 == -8

    # Test 2: foldl with initial value
    # foldl(-, [1,2,3], 10) = ((10-1)-2)-3 = 4
    arr2 = [1, 2, 3]
    r2 = foldl(mysub, arr2, 10)
    check2 = r2 == 4

    # Define add function

    # Test 3: foldl with add (same as sum)
    arr3 = [1, 2, 3, 4, 5]
    r3 = foldl(myadd, arr3)
    check3 = r3 == 15

    # All checks must pass
    @test (check1 && check2 && check3)
end

true  # Test passed
