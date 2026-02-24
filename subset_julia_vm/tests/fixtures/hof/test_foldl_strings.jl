# Test foldl with string arrays
# Issue #409: foldl with string arrays causes as_f64_vec() error

using Test

@testset "foldl with string arrays (Issue #409)" begin

    # Test with numeric operation on array length
    # This verifies the foldl machinery works with non-numeric arrays
    len_arr = ["hello", "world", "!"]
    count = foldl((acc, elem) -> acc + 1, len_arr, 0)
    println(count)  # Should print 3

    # Return true if we got this far without crashing
    @test (count == 3)
end

true  # Test passed
