# Test String array operations (Issue #811)
# - setindex! on String arrays
# - reverse on String arrays
# - reverse! on String arrays

using Test

@testset "String array setindex!" begin
    # Basic element assignment
    arr = ["a", "b", "c"]
    arr[1] = "x"
    # Use string() to ensure correct type comparison
    @test string(arr[1]) == "x"
    @test string(arr[2]) == "b"
    @test string(arr[3]) == "c"

    # Middle element assignment
    arr[2] = "y"
    @test string(arr[1]) == "x"
    @test string(arr[2]) == "y"
    @test string(arr[3]) == "c"

    # Last element assignment
    arr[3] = "z"
    @test string(arr[1]) == "x"
    @test string(arr[2]) == "y"
    @test string(arr[3]) == "z"
end

@testset "String array reverse" begin
    # reverse (non-mutating)
    arr = ["a", "b", "c"]
    rev = reverse(arr)
    @test string(rev[1]) == "c"
    @test string(rev[2]) == "b"
    @test string(rev[3]) == "a"
    # Original unchanged
    @test string(arr[1]) == "a"
    @test string(arr[2]) == "b"
    @test string(arr[3]) == "c"
end

@testset "String array reverse!" begin
    # reverse! (in-place)
    arr = ["a", "b", "c"]
    reverse!(arr)
    @test string(arr[1]) == "c"
    @test string(arr[2]) == "b"
    @test string(arr[3]) == "a"

    # Single element array
    single = ["only"]
    reverse!(single)
    @test string(single[1]) == "only"

    # Two element array
    two = ["first", "second"]
    reverse!(two)
    @test string(two[1]) == "second"
    @test string(two[2]) == "first"
end

true
