# Test dequeue operations: pushfirst!, popfirst!, insert!, append!, prepend!, deleteat!

using Test

@testset "Dequeue operations: pushfirst!, popfirst!, insert!, append!, prepend!, deleteat!" begin

    # =============================================================================
    # pushfirst! tests
    # =============================================================================

    # Test pushfirst! with integers
    a1 = [2, 3, 4]
    pushfirst!(a1, 1)
    check_pushfirst_1 = a1[1] == 1 && a1[2] == 2 && a1[3] == 3 && a1[4] == 4

    # Test pushfirst! with floats
    a2 = [2.0, 3.0]
    pushfirst!(a2, 1.0)
    check_pushfirst_2 = a2[1] == 1.0 && a2[2] == 2.0 && a2[3] == 3.0

    # Test pushfirst! returns the array
    a3 = [1, 2]
    result3 = pushfirst!(a3, 0)
    check_pushfirst_3 = result3 === a3

    # Test pushfirst! on empty array
    a4 = Int64[]
    pushfirst!(a4, 42)
    check_pushfirst_4 = length(a4) == 1 && a4[1] == 42

    # =============================================================================
    # popfirst! tests
    # =============================================================================

    # Test popfirst! returns first element
    a5 = [1, 2, 3, 4]
    val5 = popfirst!(a5)
    check_popfirst_1 = val5 == 1 && length(a5) == 3 && a5[1] == 2 && a5[2] == 3 && a5[3] == 4

    # Test popfirst! with floats
    a6 = [1.5, 2.5, 3.5]
    val6 = popfirst!(a6)
    check_popfirst_2 = val6 == 1.5 && length(a6) == 2

    # Test popfirst! on single element array
    a7 = [99]
    val7 = popfirst!(a7)
    check_popfirst_3 = val7 == 99 && length(a7) == 0

    # =============================================================================
    # insert! tests
    # =============================================================================

    # Test insert! at beginning
    a8 = [2, 3, 4]
    insert!(a8, 1, 1)
    check_insert_1 = a8[1] == 1 && a8[2] == 2 && a8[3] == 3 && a8[4] == 4

    # Test insert! in middle
    a9 = [1, 3, 4]
    insert!(a9, 2, 2)
    check_insert_2 = a9[1] == 1 && a9[2] == 2 && a9[3] == 3 && a9[4] == 4

    # Test insert! at end
    a10 = [1, 2, 3]
    insert!(a10, 4, 4)
    check_insert_3 = a10[1] == 1 && a10[2] == 2 && a10[3] == 3 && a10[4] == 4

    # Test insert! returns the array
    a11 = [1, 2]
    result11 = insert!(a11, 2, 99)
    check_insert_4 = result11 === a11

    # =============================================================================
    # append! tests
    # =============================================================================

    # Test append! with array
    a12 = [1, 2]
    append!(a12, [3, 4])
    check_append_1 = length(a12) == 4 && a12[1] == 1 && a12[2] == 2 && a12[3] == 3 && a12[4] == 4

    # Test append! with range
    a13 = [1, 2]
    append!(a13, 3:5)
    check_append_2 = length(a13) == 5 && a13[3] == 3 && a13[4] == 4 && a13[5] == 5

    # Test append! returns the array
    a14 = [1, 2]
    result14 = append!(a14, [3])
    check_append_3 = result14 === a14

    # Test append! with empty collection
    a15 = [1, 2, 3]
    append!(a15, Int64[])
    check_append_4 = length(a15) == 3

    # =============================================================================
    # prepend! tests
    # =============================================================================

    # Test prepend! with array
    a16 = [3, 4]
    prepend!(a16, [1, 2])
    check_prepend_1 = length(a16) == 4 && a16[1] == 1 && a16[2] == 2 && a16[3] == 3 && a16[4] == 4

    # Test prepend! with range
    a17 = [4, 5]
    prepend!(a17, 1:3)
    check_prepend_2 = length(a17) == 5 && a17[1] == 1 && a17[2] == 2 && a17[3] == 3

    # Test prepend! returns the array
    a18 = [2, 3]
    result18 = prepend!(a18, [1])
    check_prepend_3 = result18 === a18

    # Test prepend! with empty collection
    a19 = [1, 2, 3]
    prepend!(a19, Int64[])
    check_prepend_4 = length(a19) == 3

    # =============================================================================
    # deleteat! tests
    # =============================================================================

    # Test deleteat! from beginning
    a20 = [1, 2, 3, 4]
    deleteat!(a20, 1)
    check_deleteat_1 = length(a20) == 3 && a20[1] == 2 && a20[2] == 3 && a20[3] == 4

    # Test deleteat! from middle
    a21 = [1, 2, 3, 4]
    deleteat!(a21, 2)
    check_deleteat_2 = length(a21) == 3 && a21[1] == 1 && a21[2] == 3 && a21[3] == 4

    # Test deleteat! from end
    a22 = [1, 2, 3, 4]
    deleteat!(a22, 4)
    check_deleteat_3 = length(a22) == 3 && a22[1] == 1 && a22[2] == 2 && a22[3] == 3

    # Test deleteat! returns the array
    a23 = [1, 2, 3]
    result23 = deleteat!(a23, 2)
    check_deleteat_4 = result23 === a23

    # =============================================================================
    # Combined operations test
    # =============================================================================

    # Test using multiple operations together
    a24 = Int64[]
    push!(a24, 3)
    pushfirst!(a24, 1)
    insert!(a24, 2, 2)
    append!(a24, [4, 5])
    check_combined = length(a24) == 5 && a24[1] == 1 && a24[2] == 2 && a24[3] == 3 && a24[4] == 4 && a24[5] == 5

    # All checks must pass
    all_pushfirst = check_pushfirst_1 && check_pushfirst_2 && check_pushfirst_3 && check_pushfirst_4
    all_popfirst = check_popfirst_1 && check_popfirst_2 && check_popfirst_3
    all_insert = check_insert_1 && check_insert_2 && check_insert_3 && check_insert_4
    all_append = check_append_1 && check_append_2 && check_append_3 && check_append_4
    all_prepend = check_prepend_1 && check_prepend_2 && check_prepend_3 && check_prepend_4
    all_deleteat = check_deleteat_1 && check_deleteat_2 && check_deleteat_3 && check_deleteat_4

    @test (all_pushfirst && all_popfirst && all_insert && all_append && all_prepend && all_deleteat && check_combined)
end

true  # Test passed
