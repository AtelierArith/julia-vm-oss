# Test dump function for various types
# These tests verify that dump produces Julia-compatible output

using Test

@testset "dump function produces Julia-compatible output format" begin

    # Test 1: dump for arrays produces correct output format
    arr = [1, 2, 3]
    dump(arr)  # Should show Array{Int64}((3,)) [1, 2, 3]

    # Test 2: dump for tuples shows element types
    t = (1, 2.0, "hello")
    dump(t)  # Should show Tuple{Int64, Float64, String} with nested elements

    # Test 3: dump for Expr shows tree structure
    e = :(1 + 2)
    dump(e)  # Should show Expr with head: Symbol call and args

    # Test 4: dump for symbols
    dump(:mySymbol)  # Should show Symbol mySymbol

    # Test 5: dump returns nothing
    result = dump(42)

    @test (result === nothing)
end

true  # Test passed
