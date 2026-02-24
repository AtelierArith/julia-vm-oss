# Test @views macro
# @views transforms all array indexing A[i:j] to view(A, i:j) within an expression

using Test

@testset "@views macro basic (Issue #466)" begin
    # Create an array
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]

    # Use @views to get a view of a slice
    v = @views arr[2:4]

    # Check view indexing
    @test v[1] == 2.0
    @test v[2] == 3.0
    @test v[3] == 4.0

    # Test length
    @test length(v) == 3
end

true
