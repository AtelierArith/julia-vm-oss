# Test for issue #825: Array mutation operations (pop!, push!, resize!, unique!)
# This test specifically covers the cases mentioned in the issue

using Test

@testset "Issue #825: Array mutation operations" begin
    # Test 1: pop! on array
    a1 = [1.0, 2.0, 3.0, 4.0]
    result1 = pop!(a1)
    check1 = result1 == 4.0
    check2 = length(a1) == 3
    check3 = a1[1] == 1.0 && a1[2] == 2.0 && a1[3] == 3.0

    # Test 2: push! on array
    a2 = [1.0, 2.0, 3.0]
    result2 = push!(a2, 4.0)
    check4 = length(a2) == 4
    check5 = a2[4] == 4.0
    check6 = result2 === a2  # push! returns the array

    # Test 3: resize! on array (shrink)
    a3 = [1, 2, 3, 4]
    result3 = resize!(a3, 2)
    check7 = length(a3) == 2
    check8 = a3[1] == 1 && a3[2] == 2
    check9 = result3 === a3  # resize! returns the array

    # Test 4: unique! on array
    a4 = [1, 2, 2, 3, 3, 3]
    result4 = unique!(a4)
    check10 = length(a4) == 3
    check11 = a4[1] == 1 && a4[2] == 2 && a4[3] == 3
    check12 = result4 === a4  # unique! returns the array

    # Test 5: unique! on array literal (as mentioned in issue)
    result5 = unique!([1, 2, 2, 3, 3, 3])
    check13 = length(result5) == 3
    check14 = result5[1] == 1 && result5[2] == 2 && result5[3] == 3

    # Test 6: resize! with grow (to ensure push! works inside resize!)
    a6 = [1.0, 2.0]
    resize!(a6, 4)
    check15 = length(a6) == 4
    check16 = a6[1] == 1.0 && a6[2] == 2.0
    check17 = a6[3] == 0.0 && a6[4] == 0.0

    # Test 7: Combining operations (as might be done in Julia code)
    a7 = [5, 4, 3, 2, 1]
    push!(a7, 0)
    pop!(a7)
    pop!(a7)
    check18 = length(a7) == 4
    check19 = a7[4] == 2

    # All checks must pass
    all_checks = check1 && check2 && check3 && check4 && check5 && check6 && check7 && check8 && check9 && check10 && check11 && check12 && check13 && check14 && check15 && check16 && check17 && check18 && check19

    @test all_checks
end

true  # Test passed
