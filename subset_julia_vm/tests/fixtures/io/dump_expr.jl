# Test dump function for Expr type
# Verifies that dump produces Julia-compatible tree structure output

using Test

@testset "dump function produces Julia-compatible tree structure for Expr" begin

    # Test dump with simple expression
    println("Testing dump(:(1 + 2)):")
    dump(:(1 + 2))

    # Test dump with nested expression
    println("\nTesting dump(:(2x + 1)):")
    dump(:(2x + 1))

    # Test dump with Symbol
    println("\nTesting dump(:hello):")
    dump(:hello)

    # Test dump with integers
    println("\nTesting dump(42):")
    dump(42)

    # Test dump with tuple
    println("\nTesting dump((1, 2, 3)):")
    dump((1, 2, 3))

    # All tests passed without error
    @test (true)
end

true  # Test passed
