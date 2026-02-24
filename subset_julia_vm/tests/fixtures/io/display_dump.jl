# Test display and dump functions

using Test

@testset "display and dump output functions" begin

    # Test display with different types
    println("Testing display:")
    display(42)
    display(3.14)
    display("hello")

    # Test dump with primitives
    println("\nTesting dump:")
    dump(42)
    dump(3.14)
    dump("hello")

    # Test dump with array
    arr = [1, 2, 3]
    dump(arr)

    # Return true for fixture test
    @test (true)
end

true  # Test passed
